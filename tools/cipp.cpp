#include <chrono>
#include <cmath>
#include <iostream>
#include <list>
#include <set>
#include <sstream>
#include <thread>
#include <vector>

#include <sys/ioctl.h>
#include <stdlib.h>

#include "perf.h"

constexpr int BW_PERCENTILE = 90;
constexpr int MAX_STEP = 8;
constexpr int MIN_STEP = 2;
constexpr int THROTTLE_THRESHOLD = 15;

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
    static int last_step = -MAX_STEP * 2;
    int64_t cur_bw;
    int nth_percentile_index;
    int bw_change, interleave_change;
    int cur_step;
    bool good_step;
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

    // Step is goof is the change is bw is less than the change in interleave ratio
    // However, if the bw change is positive (bw going down) when the interleave change
    // is negative (more in local), we probably want to stay in the same direction
    good_step = (bw_change < interleave_change) || (bw_change > 0 && interleave_change < 0);

    // Adjust the interleave ratio
    if (cur_bw < bw_cutoff) {
        // The bandwidth is clearly unsaturated, so increase the local ratio
        if (last_step == 0 && bw_change > 0) {
            cur_step = (ratio * (bw_change / 100)) / 100;
            if (abs(cur_step) < MIN_STEP)
                cur_step = MIN_STEP;
            else if (abs(cur_step) > MAX_STEP / 2)
                cur_step = (cur_step > 0) ? MAX_STEP / 2 : -MAX_STEP / 2;
            correct_count = 0;
        } else if (last_step <= 0) {
            cur_step = std::max(abs(last_step) / 2, MIN_STEP);
            correct_count = 0;
        } else {
            cur_step = last_step;
            correct_count++;
        }
    } else if (last_step == 0) {
        // If we have stopped moving, see if the bandwidth has changed
        // enough due to application changes to search again.
        // Divide by 100 because bw_change is in houndreths of a percent
        cur_step = (ratio * (bw_change / 100)) / 100;
        if (abs(cur_step) < 4)
            cur_step = 0;
        else if (abs(cur_step) > MAX_STEP / 2)
            cur_step = (cur_step > 0) ? MAX_STEP / 2 : -MAX_STEP / 2;

        correct_count = 0;
    } else if (last_ratio == 100) {
        // Probe downward to see if we can make use of more bandwidth
        cur_step = -abs(last_step) / 2;
        correct_count = 0;
    } else if (good_step) {
        int bw_int_ratio = (bw_change * 100 / 2) / interleave_change;
        if (bw_change > 0 && interleave_change > 0 && bw_int_ratio > THROTTLE_THRESHOLD) {
            cur_step = (100 - bw_int_ratio) * last_step / 100;
        } else {
            // The last step was good, keep going
            correct_count++;
            cur_step = last_step;
        }
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

    // If this is the first step we've gone to a step size
    // of 0, steady ourselves at the better option
    if (cur_step == 0 && last_bw > cur_bw)
        ratio = last_ratio;
    else
        ratio += cur_step;
    last_step = cur_step;

    if (ratio > 100)
        ratio = 100;
    else if (ratio < 0)
        ratio = 0;

    // Actually change the ratio
    shell_cmd << "echo " << ratio << " | tee /sys/kernel/mm/mempolicy/weighted_interleave/node0";
    system(shell_cmd.str().c_str());
    shell_cmd.str(std::string());
    shell_cmd << "echo " << 100 - ratio << " | tee /sys/kernel/mm/mempolicy/weighted_interleave/node1";
    system(shell_cmd.str().c_str());

    std::cout << "Target ratio: " << ratio << " "
              << "BW Change: " << bw_change << " "
              << "Int Change: " << interleave_change << " "
              << "BW: " << cur_bw << std::endl;

    return ratio;
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
    int sample_interval_ms;
    int adjust_interval_ms;
    uint64_t max_list_size;
    int64_t cur_bw;
    int64_t bw_saturation_cutoff;
    int interleave_ratio = 100;

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
            interleave_ratio = adjust_interleave_ratio(bw_history, interleave_ratio, bw_saturation_cutoff);
            bw_history.clear();
        }
    }
}
