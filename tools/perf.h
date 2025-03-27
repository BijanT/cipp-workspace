#ifndef _PERF_H
#define _PERF_H

#include <fstream>
#include <vector>
#include <cstdint>

#include <linux/perf_event.h>

#ifdef GNR
extern std::vector<uint32_t> cxl_types;
extern std::vector<uint64_t> cxl_read_configs;
extern std::vector<uint64_t> cxl_write_configs;
#endif

int perf_event_open(struct perf_event_attr *attr, pid_t pid, int cpu,
    int group_fd, unsigned long flags);
void get_perf_uncore_info(std::vector<uint32_t> &types, std::vector<int> &cpus,
    std::vector<uint64_t> &rd_configs, std::vector<uint64_t> &wr_configs);
void open_perf_events(int cpu, std::vector<uint32_t> types,
    std::vector<uint64_t> &configs, std::vector<int> &fds);
int perf_sample_open(pid_t pid, int cpu, int group_fd, uint64_t type, uint64_t config,
    uint64_t config1, uint64_t sample_type, uint64_t sample_period);
void apply_ioctl(int cmd, std::vector<int> fds);

#endif
