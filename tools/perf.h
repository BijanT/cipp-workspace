#ifndef _PERF_H
#define _PERF_H

#include <fstream>
#include <vector>

#include <linux/perf_event.h>

struct perf_sample {
    struct perf_event_header header;
    pid_t pid;
    pid_t tid;
    uint64_t addr;
    uint64_t phys_addr;
};

int perf_event_open(struct perf_event_attr *attr, pid_t pid, int cpu,
    int group_fd, unsigned long flags);
void get_perf_uncore_info(std::vector<uint32_t> &types, std::vector<int> &cpus,
    std::vector<uint64_t> &rd_configs, std::vector<uint64_t> &wr_configs);
void open_perf_events(int cpu, std::vector<uint32_t> types,
    std::vector<uint64_t> &configs, std::vector<int> &fds);
struct perf_event_mmap_page *perf_sample_setup(pid_t pid, int cpu, uint64_t type, uint64_t config,
    uint64_t config1, uint64_t sample_period, int *out_fd);
void apply_ioctl(int cmd, std::vector<int> fds);

#endif
