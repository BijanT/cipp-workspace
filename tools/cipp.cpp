#include <atomic>
#include <chrono>
#include <cmath>
#include <iostream>
#include <list>
#include <map>
#include <set>
#include <sstream>
#include <thread>
#include <vector>

#include <numaif.h>
#include <sys/ioctl.h>
#include <stdlib.h>

#include "perf.h"

constexpr int PAGE_SIZE = 4096;
constexpr int HPAGE_SIZE = (512 * PAGE_SIZE);
constexpr int PAGE_SHIFT = 12;
constexpr int HPAGE_SHIFT = 21;

constexpr float COUNT_DAMP_FACTOR = 0.67;

constexpr int BW_PERCENTILE = 80;
constexpr int MAX_STEP = 10;
constexpr int MIN_STEP = 2;

constexpr int KPF_SIZE = 8;
constexpr uint64_t KPF_ANON = ((uint64_t)1 << 12);
constexpr uint64_t KPF_THP = ((uint64_t)1 << 22);
constexpr uint64_t MEM_LOAD_RETIRED_L3_MISS = 0x20d1;

struct page_info {
    uint32_t count;
    uint8_t addr_mod_100;
    bool huge;
};

std::atomic_int global_int_ratio = 100;

int64_t get_bw(int sample_int, std::vector<int> &rd_fds, std::vector<int> wr_fds)
{
    uint64_t count;
    int64_t rd_bw, wr_bw;
    uint64_t rd_count = 0;
    uint64_t wr_count = 0;

    apply_ioctl(PERF_EVENT_IOC_RESET, rd_fds);
    apply_ioctl(PERF_EVENT_IOC_RESET, wr_fds);
    apply_ioctl(PERF_EVENT_IOC_ENABLE, rd_fds);
    apply_ioctl(PERF_EVENT_IOC_ENABLE, wr_fds);

    auto start_time = std::chrono::high_resolution_clock::now();

    std::this_thread::sleep_for(std::chrono::milliseconds(sample_int));

    apply_ioctl(PERF_EVENT_IOC_DISABLE, rd_fds);
    apply_ioctl(PERF_EVENT_IOC_DISABLE, wr_fds);

    auto end_time = std::chrono::high_resolution_clock::now();
    auto duration = std::chrono::duration_cast<std::chrono::microseconds>(end_time - start_time);

    for (int fd : rd_fds) {
        read(fd, &count, sizeof(count));

        rd_count += count;
    }
    for (int fd : wr_fds) {
        read(fd, &count, sizeof(count));

        wr_count += count;
    }

    // bw in MB/s = (bytes read / 1,000,000) / (time in seconds)
    //            = (bytes read / 1,000,000) / (time in us / 1,000,000)
    //            = (bytes read) / (time in us)
    rd_bw = (rd_count * 64) / duration.count();
    wr_bw = (wr_count * 64) / duration.count();

    return rd_bw + wr_bw;
}

int adjust_interleave_ratio(std::list<int64_t> &bw_history, int ratio, int64_t bw_cutoff)
{
    static int correct_count = 0;
    static int64_t last_bw = 0;
    static int last_ratio = 100;
    static int last_step = -MAX_STEP;
    int64_t cur_bw;
    int nth_percentile_index;
    int bw_change, interleave_change;
    int cur_step;
    std::stringstream shell_cmd;
    std::multiset<uint64_t> sorted_bw;
    long unsigned int i = 0;

    // Sort the bandwidths to get the Nth percentile
    for (uint64_t bw : bw_history) {
        i++;
        // Throw away the earlist half of the samples to account for DAMON
        // not migrating the pages immediately
        if (i < bw_history.size() / 2)
            continue;
        sorted_bw.insert(bw);
    }
    nth_percentile_index = (sorted_bw.size() * BW_PERCENTILE / 100) - 1;
    cur_bw = *std::next(sorted_bw.begin(), nth_percentile_index);
    if (cur_bw == 0)
	    cur_bw = 1;

    // Calculate the relative change in BW and interleave ratio
    if (last_bw == 0)
        last_bw = cur_bw;
    if (last_ratio == 0)
        last_ratio = 1;
    // Multiple by 10000 instead of 100 to get more resolution
    bw_change = (10000 * (last_bw - cur_bw)) / last_bw;
    interleave_change = last_step * -100;//(10000 * (last_ratio - ratio)) / last_ratio;

    last_ratio = ratio;

    // Adjust the interleave ratio
    if (cur_bw < bw_cutoff) {
        // The bandwidth is clearly unsaturated, so increase the local ratio
        if (last_step <= 0) {
            cur_step = std::max(abs(last_step) / 2, MIN_STEP);
            correct_count = 0;
        } else {
            cur_step = last_step;
            correct_count++;
        }
    } else if (last_ratio == 100) {
        // Probe downward to see if we can make use of more bandwidth
        cur_step = -abs(last_step);
        correct_count = 0;
    } else if (last_step == 0) {
        // If we have stopped moving, see if the bandwidth has changed
        // enough due to application changes to search again.
        // Divide by 100 because bw_change is in houndreths of a percent
        cur_step = bw_change / 100;
        if (abs(cur_step) < 4)
            cur_step = 0;
        correct_count = 0;
    } else if (bw_change < interleave_change / 2) {
        // The last step was good, keep going
        correct_count++;
        cur_step = last_step;
    } else {
        // The last step was bad, reverse
        correct_count = 0;
        cur_step = -last_step / 2;
    }

    // If we've been correct multiple times in a row, we might be far
    // away from the ideal, so pick up the pace!
    if (correct_count >= 3) {
        cur_step = cur_step * 2;
        correct_count = 0;
    }

    // Make sure the step stays in bounds
    if (abs(cur_step) < MIN_STEP) {
        cur_step = 0;
    } else if (abs(cur_step) > MAX_STEP) {
        cur_step = cur_step < 0 ? -MAX_STEP : MAX_STEP;
    }

    if (last_step != 0 || cur_step != 0)
        last_bw = cur_bw;

    ratio += cur_step;
    last_step = cur_step;

    if (ratio > 100)
        ratio = 100;
    else if (ratio < 0)
        ratio = 0;

    // Actually change the ratio
    if (ratio == 100) {
        // Setting these files to 0 actually sets them to 1, so set node 0 to
        // max, and node 1 to 1
        shell_cmd << "echo 255 | tee /sys/kernel/mm/mempolicy/weighted_interleave/node0";
        system(shell_cmd.str().c_str());
        shell_cmd.str(std::string());
        shell_cmd << "echo 1 | tee /sys/kernel/mm/mempolicy/weighted_interleave/node1";
        system(shell_cmd.str().c_str());
    } else {
        shell_cmd << "echo " << ratio << " | tee /sys/kernel/mm/mempolicy/weighted_interleave/node0";
        system(shell_cmd.str().c_str());
        shell_cmd.str(std::string());
        shell_cmd << "echo " << 100 - ratio << " | tee /sys/kernel/mm/mempolicy/weighted_interleave/node1";
        system(shell_cmd.str().c_str());
    }

    std::cout << "Target ratio: " << ratio << " "
              << "BW Change: " << bw_change << " "
              << "Int Change: " << interleave_change << " "
              << "BW: " << cur_bw << std::endl;

    return ratio;
}

uint64_t read_from_file(FILE *f) {
    uint8_t c;
    uint64_t value = 0;

    for (int i = 0; i < KPF_SIZE; i++) {
        c = fgetc(f);
        value += (((uint64_t)c) << (i*8));
    }

    return value;
}

void migrate_pages(pid_t pid, std::map<uint64_t, page_info> &page_infos,
        void **pages, int *nodes, int *status, long size)
{
    long pages_to_move;
    long count;
    long ret;
    int dest_node;

    pages_to_move = page_infos.size();
    auto it = page_infos.begin();

    while (pages_to_move > 0) {
        count = 0;

        while (count < size && it != page_infos.end()) {
            if (it->second.addr_mod_100 < global_int_ratio)
                dest_node = 0;
            else
                dest_node = 1;

            pages[count] = (void*)it->first;
            nodes[count] = dest_node;
            count++;
            it++;
        }

        ret = move_pages(pid, count, pages, nodes, status, MPOL_MF_MOVE);
        if (ret != 0) {
            std::cout << "Error moving pages: " << errno << std::endl;
        }

        pages_to_move -= count;
    }
}

void migrator_thread(int migrate_interval_ms)
{
    // The maximum number of pages to migrate in one call
    constexpr uint64_t MAX_MIGRATE = 100000;
    const int num_cpus = std::thread::hardware_concurrency();
    std::map<pid_t, std::map<uint64_t, page_info>> pages_list;
    FILE *kpf_file;
    std::vector<struct perf_event_mmap_page*> pebs;
    std::vector<int> fds;
    std::vector<void*> pages;
    std::vector<int> nodes;
    std::vector<int> status;

    pages.resize(MAX_MIGRATE);
    nodes.resize(MAX_MIGRATE);
    status.resize(MAX_MIGRATE);

    kpf_file = fopen("/proc/kpageflags", "rb");
    if (!kpf_file) {
        std::cout << "Unable to open /proc/kpageflags" << std::endl;
        return;
    }

    std::cout << "Setting up PEBS for " << num_cpus << " cores" << std::endl;

    for (int i = 0; i < num_cpus; i++) {
        int fd;
        pebs.push_back(perf_sample_setup(-1, i, PERF_TYPE_RAW, MEM_LOAD_RETIRED_L3_MISS,
            0, 5000, &fd));
        fds.push_back(fd);
        if (!pebs[i]) {
            std::cout << "Error setting up PEBS! " << errno << std::endl;
            return;
        }
    }

    auto start_time = std::chrono::high_resolution_clock::now();
    while (true) {
        struct perf_event_header *ph;
        struct perf_sample *ps;
        uint64_t sample_addr;
        uint64_t pfn;
        uint64_t kpf_entry;
        pid_t pid;
        int ret;
        bool huge;
        bool read_sample = false;

        // Every so often, migrate the pages
        auto now = std::chrono::high_resolution_clock::now();
        auto duration = std::chrono::duration_cast<std::chrono::milliseconds>(now - start_time);
        if (duration.count() >= migrate_interval_ms) {
            apply_ioctl(PERF_EVENT_IOC_DISABLE, fds);

            // TODO

            for (auto& [pid, proc_pages] : pages_list) {
                migrate_pages(pid, proc_pages, pages.data(), nodes.data(), status.data(), MAX_MIGRATE);

                // Dampen the counts in the page list
                auto it = proc_pages.begin();
                while (it != proc_pages.end()) {
                    it->second.count *= COUNT_DAMP_FACTOR;
                    if (it->second.count == 0)
                        it = proc_pages.erase(it);
                    else
                        it++;
                }
            }

            apply_ioctl(PERF_EVENT_IOC_ENABLE, fds);
            start_time = std::chrono::high_resolution_clock::now();
        }

        for (struct perf_event_mmap_page *p : pebs) {
            uint8_t *pbuf = (uint8_t*)p + p->data_offset;

            // This was in the reference code. I assume it's needed
            __sync_synchronize();

            // Do we have new events?
            if (p->data_head == p->data_tail) {
                continue;
            }
            read_sample = true;

            // Read the event
            ph = (struct perf_event_header*)(pbuf + (p->data_tail % p->data_size));
            switch (ph->type) {
            case PERF_RECORD_SAMPLE:
                ps = (struct perf_sample*)ph;

                pid = ps->pid;
                sample_addr = ps->addr;
                pfn = ps->phys_addr >> PAGE_SHIFT;
                break;
            case PERF_RECORD_THROTTLE:
            case PERF_RECORD_UNTHROTTLE:
                break;
            default:
                std::cout << "Unknown type " << ph->type << std::endl;
                break;
            }
            p->data_tail += ph->size;

            // Is this a valid address?
            if (!sample_addr)
                continue;

            // Get the page stats
            ret = fseek(kpf_file, pfn * KPF_SIZE, SEEK_SET);
            if (ret) {
                std::cout << "Failed to fseek kpf!" << std::endl;
                return;
            }
            kpf_entry = read_from_file(kpf_file);

            // Only care about anon pages
            if (!(kpf_entry & KPF_ANON))
                continue;

            if (kpf_entry & KPF_THP) {
                huge = true;
                sample_addr = sample_addr & ~(HPAGE_SIZE - 1);
            } else {
                huge = false;
                sample_addr = sample_addr & ~(PAGE_SIZE - 1);
            }

            // Update the page count
            auto it = pages_list[pid].find(sample_addr);
            if (it == pages_list[pid].end()) {
                uint64_t shifted_addr;

                if (huge)
                    shifted_addr = sample_addr >> HPAGE_SHIFT;
                else
                    shifted_addr = sample_addr >> PAGE_SHIFT;

                pages_list[pid][sample_addr].addr_mod_100 = shifted_addr % 100;
                pages_list[pid][sample_addr].count = 1;
                pages_list[pid][sample_addr].huge = huge;
            } else {
                pages_list[pid][sample_addr].count++;
            }
        }

        if (!read_sample)
            std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
}

int main(int argc, char *argv[])
{
    std::vector<uint32_t> types;
    std::vector<int> cpus;
    std::vector<uint64_t> rd_configs;
    std::vector<uint64_t> wr_configs;
    std::vector<int> rd_fds;
    std::vector<int> wr_fds;
    std::list<int64_t> bw_history;
    std::thread mig_thread;
    int sample_interval_ms;
    int adjust_interval_ms;
    uint64_t max_list_size;
    int64_t cur_bw;
    int64_t bw_saturation_cutoff;

    if (argc < 4) {
        std::cout << "Usage: ./cipp <sample int (ms)> <adjust int (ms)> <bw saturation cutoff (MB/s)>" << std::endl;
        return -1;
    }

    sample_interval_ms = std::stoi(argv[1]);
    adjust_interval_ms = std::stoi(argv[2]);
    bw_saturation_cutoff = std::stoul(argv[3]);

    max_list_size = adjust_interval_ms / sample_interval_ms;

    std::cout << "Running with " << std::endl
        << "\tSample interval: " << sample_interval_ms << " ms" << std::endl
        << "\tAdjust interval: " << adjust_interval_ms << " ms" << std::endl
        << "\tBandwidth saturation cutoff: " << bw_saturation_cutoff << " MB/s" << std::endl;

    if (argc == 5)
        mig_thread = std::thread(migrator_thread, 1000);

    get_perf_uncore_info(types, cpus, rd_configs, wr_configs);

    // What to put in the cpu to read from each socket can be found by reading
    // /sys/devices/uncore_imc_0/cpumask - we only care about socket 0, which
    // is represented by CPU 0
    open_perf_events(0, types, rd_configs, rd_fds);
    open_perf_events(0, types, wr_configs, wr_fds);

    while (true) {
        cur_bw = get_bw(sample_interval_ms, rd_fds, wr_fds);

        bw_history.push_back(cur_bw);

        // Have we reached an adjustment interval?
        if (bw_history.size() >= max_list_size) {
            global_int_ratio = adjust_interleave_ratio(bw_history, global_int_ratio, bw_saturation_cutoff);
            bw_history.clear();
        }
    }
}
