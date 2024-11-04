#ifndef _PERF_H
#define _PERF_H

#include <fstream>
#include <vector>

#include <linux/perf_event.h>

int perf_event_open(struct perf_event_attr *attr, pid_t pid, int cpu,
    int group_fd, unsigned long flags);
void get_perf_uncore_info(std::vector<uint32_t> &types, std::vector<uint64_t> &rd_configs,
    std::vector<uint64_t> &wr_configs);
void open_perf_events(int cpu, std::vector<uint32_t> types,
    std::vector<uint64_t> &configs, std::vector<int> &fds);
void apply_ioctl(int cmd, std::vector<int> fds);

#endif
