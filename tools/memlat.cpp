#include <chrono>
#include <fstream>
#include <iostream>
#include <string>
#include <sstream>
#include <thread>
#include <vector>

#include <linux/perf_event.h>
#include <numa.h>

#include "perf.h"

#define PAGE_SHIFT 12
#define MEM_TRANS_RETIRED 0x01CD
#define SAMPLE_PERIOD 5000
#define EWMA_EXP 1

struct perf_sample {
    struct perf_event_header header;
    union perf_sample_weight weight;
    uint64_t data_src;
    uint64_t phys_addr;
};

int main(int argc, char* argv[])
{
    int agg_interval;
    int agg_count = 0;
    uint64_t remote_pfn;
    struct bitmask *cpumask;
    uint64_t local_lat_sum = 0;
    uint64_t local_lat_count = 0;
    uint64_t remote_lat_sum = 0;
    uint64_t remote_lat_count = 0;
    uint64_t smoothed_local_lat = 0;
    uint64_t smoothed_remote_lat = 0;
    std::vector<int> cpus;
    std::vector<int> perf_fds;
    std::vector<struct perf_event_mmap_page*> pebs;
    std::streambuf *buf;
    std::ofstream out_file;

    if (argc != 3 && argc != 4) {
        std::cerr << "Usage: ./memlat <remote pfn start> <Aggregation Interval (ms)> <out file>" << std::endl;
        return -1;
    }

    remote_pfn = atol(argv[1]);
    if (!remote_pfn) {
        std::cerr << "Invalid remote pfn start: " << argv[1] << std::endl;
        return -1;
    }

    agg_interval = atoi(argv[2]);
    if (!agg_interval) {
        std::cerr << "Invalid sample interval: " << argv[2] << std::endl;
        return -1;
    }

    if (argc == 3) {
        buf = std::cout.rdbuf();
    } else {
        out_file.open(argv[2]);
        if (!out_file.is_open()) {
            std::cerr << "Could not open " << argv[2] << "for writting" << std::endl;
            return -1;
        }
        buf = out_file.rdbuf();
    }
    std::ostream out(buf);

    cpumask = numa_allocate_cpumask();
    if (!cpumask) {
        std::cerr << "Could not allocate cpumask" << std::endl;
        return -1;
    }

    if (numa_node_to_cpus(0, cpumask)) {
        std::cerr << "Error reading node 0 CPUs" << std::endl;
        return -1;
    }

    for (long unsigned int i = 0; i < cpumask->size; i++) {
        if (numa_bitmask_isbitset(cpumask, i)) {
            cpus.push_back(i);
        }
    }

    for (int cpu : cpus) {
        int fd;
        struct perf_event_mmap_page *p;
        // Minimum latency in cycles to sample
        uint64_t ldlat = 300;
        uint64_t sample_type = PERF_SAMPLE_PHYS_ADDR | PERF_SAMPLE_WEIGHT_STRUCT | PERF_SAMPLE_DATA_SRC;

        p = perf_sample_setup(-1, cpu, PERF_TYPE_RAW, MEM_TRANS_RETIRED, ldlat,
            sample_type, SAMPLE_PERIOD, &fd);
        if (!p) {
            std::cerr << "Error setting up PEBS: " << errno << std::endl;
            return -1;
        }

        pebs.push_back(p);
        perf_fds.push_back(fd);
    }

    auto start_time = std::chrono::high_resolution_clock::now();
    while (true) {
        struct perf_event_header *ph;
        struct perf_sample *ps;
        uint64_t pfn;
        bool read_sample = false;

        // Every so often, collect the results
        auto now = std::chrono::high_resolution_clock::now();
        auto duration = std::chrono::duration_cast<std::chrono::milliseconds>(now - start_time);
        if (duration.count() >= agg_interval) {
            int local_lat = 0;
            int remote_lat = 0;

            agg_count++;

            apply_ioctl(PERF_EVENT_IOC_DISABLE, perf_fds);

            if (local_lat_count > 0)
                local_lat = local_lat_sum / local_lat_count;
            if (remote_lat_count > 0)
                remote_lat = remote_lat_sum / remote_lat_count;

            smoothed_local_lat = (local_lat + ((1<<EWMA_EXP) - 1)*smoothed_local_lat)>>EWMA_EXP;
            smoothed_remote_lat = (remote_lat + ((1<<EWMA_EXP) - 1)*smoothed_remote_lat)>>EWMA_EXP;

            // To not overwhelm the reader,  only print occasionally
            if (agg_count % 10 == 0) {
                out << "Local " << smoothed_local_lat << " Remote " << smoothed_remote_lat << std::endl;
                out << local_lat_count << " " << remote_lat_count << std::endl;
            }

            local_lat_sum = local_lat_count = 0;
            remote_lat_sum = remote_lat_count = 0;
            apply_ioctl(PERF_EVENT_IOC_ENABLE, perf_fds);
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

            if (!pfn)
                continue;

            if (pfn < remote_pfn) {
                local_lat_sum += ps->weight.var1_dw;
                local_lat_count++;
            } else {
                remote_lat_sum += ps->weight.var1_dw;
                remote_lat_count++;
            }
        }

        if (!read_sample)
            std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }

    numa_free_cpumask(cpumask);
}
