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
    std::vector<uint64_t> rd_configs;
    std::vector<uint64_t> wr_configs;
    std::vector<int> rd_fds;
    std::vector<int> wr_fds;
    std::streambuf *buf;
    std::ofstream out_file;
    int sample_interval_ms;
    uint64_t rd_count, wr_count;
    uint64_t rd_bw, wr_bw;
    uint64_t count;
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

    get_perf_uncore_info(types, rd_configs, wr_configs);

    // What to put in the cpu to read from each socket can be found by reading
    // /sys/devices/uncore_imc_0/cpumask - we only care about socket 0, which
    // is represented by CPU 0
    open_perf_events(0, types, rd_configs, rd_fds);
    open_perf_events(0, types, wr_configs, wr_fds);

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
        rd_count = wr_count = 0;

        apply_ioctl(PERF_EVENT_IOC_RESET, rd_fds);
        apply_ioctl(PERF_EVENT_IOC_RESET, wr_fds);
        apply_ioctl(PERF_EVENT_IOC_ENABLE, rd_fds);
        apply_ioctl(PERF_EVENT_IOC_ENABLE, wr_fds);

        std::this_thread::sleep_for(std::chrono::milliseconds(sample_interval_ms));

        apply_ioctl(PERF_EVENT_IOC_DISABLE, rd_fds);
        apply_ioctl(PERF_EVENT_IOC_DISABLE, wr_fds);

        for (int fd : rd_fds) {
            read(fd, &count, sizeof(count));

            rd_count += count;
        }
        for (int fd : wr_fds) {
            read(fd, &count, sizeof(count));

            wr_count += count;
        }

        rd_bw = (rd_count * 64) / (sample_interval_ms * 1000);
        wr_bw = (wr_count * 64) / (sample_interval_ms * 1000);

        out << "Read " << rd_bw << " Write " << wr_bw << " Total "
            << rd_bw + wr_bw << " MB/s" << std::endl;
    }

    return 0;
}

