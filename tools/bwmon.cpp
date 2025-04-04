#include <chrono>
#include <fstream>
#include <iostream>
#include <string>
#include <sstream>
#include <thread>
#include <vector>

#include <linux/perf_event.h>
#include <sys/ioctl.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <cassert>
#include <cstring>
#include <cstdlib>
#include <poll.h>
#include <unistd.h>

#include "perf.h"

#ifndef __NR_pidfd_open
#define __NR_pidfd_open 434
#endif

int pidfd_open(pid_t pid, unsigned int flags) {
    return syscall(__NR_pidfd_open, pid, flags);
}

int main(int argc, char* argv[])
{
    std::vector<uint32_t> types;
    std::vector<int> cpus;
    std::vector<uint64_t> rd_configs;
    std::vector<uint64_t> wr_configs;
    std::vector<std::vector<int>> rd_fds;
    std::vector<std::vector<int>> wr_fds;
    std::streambuf *buf;
    std::ofstream out_file;
    int sample_interval_ms;
    uint64_t rd_count, wr_count;
    uint64_t rd_bw, wr_bw;
    uint64_t total_bw;
    uint64_t count;
    long unsigned int i;
	long unsigned int num_nodes;
    struct pollfd pollfd;
    pid_t pid;
    int pidfd;

    if (argc != 2 && argc < 4) {
        std::cerr << "Usage: ./bwmon <Sample Interval (ms)> <out file> <pid>" << std::endl;
        return -1;
    }

    sample_interval_ms = atoi(argv[1]);
    if (!sample_interval_ms) {
        std::cout << "Invalid sample interval: " << argv[1] << std::endl;
        return -1;
    }

    if (argc == 2) {
        buf = std::cout.rdbuf();
    } else {
        out_file.open(argv[2]);
        if (!out_file.is_open()) {
            std::cerr << "Could not open " << argv[2] << " for writting" << std::endl;
            return -1;
        }
        buf = out_file.rdbuf();
    }
    std::ostream out(buf);

    get_perf_uncore_info(types, cpus, rd_configs, wr_configs);

#ifdef GNR
	num_nodes = cpus.size() + 1;
#else
	num_nodes = cpus.size();
#endif
    rd_fds.resize(num_nodes);
    wr_fds.resize(num_nodes);
    for (i = 0; i < cpus.size(); i++) {
        open_perf_events(cpus[i], types, rd_configs, rd_fds[i]);
        open_perf_events(cpus[i], types, wr_configs, wr_fds[i]);
    }
#ifdef GNR
    // Quick hack: The performance counters for CXL are different than for local.
    // Just use hardcoded values
    open_perf_events(0, cxl_types, cxl_read_configs, rd_fds[i]);
    open_perf_events(0, cxl_types, cxl_write_configs, wr_fds[i]);
#endif

    if (argc == 2) {
        pid = -1;
    } else {
        pid = atoi(argv[3]);

        pidfd = pidfd_open(pid, 0);
        if (pidfd < 0) {
            std::cerr << "Could not open pidfd for " << pid << std::endl;
            return -1;
        }

        pollfd.fd = pidfd;
        pollfd.events = POLLIN;
    }

    while (true) {
        total_bw = 0;

        // If tracking a proccess, see if it has exited
        if (pid != -1) {
            if (poll(&pollfd, 1, 0) != 0)
                break;
        }

        for (i = 0; i < num_nodes; i++) {
            apply_ioctl(PERF_EVENT_IOC_RESET, rd_fds[i]);
            apply_ioctl(PERF_EVENT_IOC_RESET, wr_fds[i]);
            apply_ioctl(PERF_EVENT_IOC_ENABLE, rd_fds[i]);
            apply_ioctl(PERF_EVENT_IOC_ENABLE, wr_fds[i]);
        }

        std::this_thread::sleep_for(std::chrono::milliseconds(sample_interval_ms));

        for (i = 0; i < num_nodes; i++) {
            apply_ioctl(PERF_EVENT_IOC_DISABLE, rd_fds[i]);
            apply_ioctl(PERF_EVENT_IOC_DISABLE, wr_fds[i]);
        }

        for (i = 0; i < num_nodes; i++) {
            rd_count = wr_count = 0;

            for (int fd : rd_fds[i]) {
                read(fd, &count, sizeof(count));

                rd_count += count;
            }
            for (int fd : wr_fds[i]) {
                read(fd, &count, sizeof(count));

                wr_count += count;
            }

            rd_bw = (rd_count * 64) / (sample_interval_ms * 1000);
            wr_bw = (wr_count * 64) / (sample_interval_ms * 1000);

            out << "Node " << i << ": Read " << rd_bw << " Write " << wr_bw << " Total "
                << rd_bw + wr_bw << " MB/s" << std::endl;

            total_bw += rd_bw + wr_bw;
        }

        out << "Aggregate BW: " << total_bw << std::endl << std::endl;
    }

    return 0;
}

