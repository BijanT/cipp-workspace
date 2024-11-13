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
#include <unistd.h>

#include "perf.h"

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
    pid_t pid;

    if (argc != 2 && argc < 4) {
        std::cerr << "Usage: ./bwmon <Sample Interval (ms)> <out file> <cmd> <args>" << std::endl;
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

    rd_fds.resize(cpus.size());
    wr_fds.resize(cpus.size());
    for (i = 0; i < cpus.size(); i++) {
        open_perf_events(cpus[i], types, rd_configs, rd_fds[i]);
        open_perf_events(cpus[i], types, wr_configs, wr_fds[i]);
    }

    if (argc == 2) {
        pid = -1;
    } else {
        pid = fork();
        if (pid == -1) {
            std::cerr << "Error forking proc: " << errno << std::endl;
            return -1;
        } else if (pid == 0) {
            execvp(argv[3], &argv[3]);
            std::cerr << "Error execing file!" << std::endl;
            return -1;
        }
    }

    while (!waitpid(pid, nullptr, WNOHANG) || pid == -1) {
        total_bw = 0;

        for (i = 0; i < cpus.size(); i++) {
            apply_ioctl(PERF_EVENT_IOC_RESET, rd_fds[i]);
            apply_ioctl(PERF_EVENT_IOC_RESET, wr_fds[i]);
            apply_ioctl(PERF_EVENT_IOC_ENABLE, rd_fds[i]);
            apply_ioctl(PERF_EVENT_IOC_ENABLE, wr_fds[i]);
        }

        std::this_thread::sleep_for(std::chrono::milliseconds(sample_interval_ms));

        for (i = 0; i < cpus.size(); i++) {
            apply_ioctl(PERF_EVENT_IOC_DISABLE, rd_fds[i]);
            apply_ioctl(PERF_EVENT_IOC_DISABLE, wr_fds[i]);
        }

        for (i = 0; i < cpus.size(); i++) {
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

