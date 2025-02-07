#include <cassert>
#include <cstring>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include <linux/perf_event.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/syscall.h>
#include <unistd.h>

#include "perf.h"

int perf_event_open(struct perf_event_attr *attr, pid_t pid, int cpu,
    int group_fd, unsigned long flags)
{
    return syscall(SYS_perf_event_open, attr, pid, cpu, group_fd, flags);
}

uint64_t read_perf_event(std::ifstream &file)
{
    std::string input;
    std::string event_str, umask_str;
    size_t start_pos, end_pos;
    uint64_t event;
    uint64_t umask;

    // Format: event=<event>,umask=<umask>
    file >> input;

    // Read the event str
    start_pos = input.find("=") + 1;
    end_pos = input.find(",");
    event_str = input.substr(start_pos, end_pos - start_pos);
    input.erase(0, end_pos + 1);

    // Read the umask string
    start_pos = input.find("=") + 1;
    umask_str = input.substr(start_pos);

    event = stol(event_str, nullptr, 16);
    umask = stol(umask_str, nullptr, 16);

    return (umask << 8) | event;
}

void get_perf_uncore_info(std::vector<uint32_t> &types, std::vector<int> &cpus,
    std::vector<uint64_t> &rd_configs, std::vector<uint64_t> &wr_configs)
{
    const std::string BASE_DIR = "/sys/devices/uncore_imc_";
    uint32_t type;
    uint64_t rd_config;
    uint64_t wr_config;
    int valid_uncore = -1;

    // I've seen the uncore_imc_ values go from 0 to 11 with gaps, so try all of them
    for (int i = 0; i < 12; i++) {
        std::stringstream type_path;
        std::stringstream read_event_path;
        std::stringstream write_event_path;
        std::ifstream type_file;
        std::ifstream read_event_file;
        std::ifstream write_event_file;

        type_path << BASE_DIR << i << "/type";
        read_event_path << BASE_DIR << i << "/events/cas_count_read";
        write_event_path << BASE_DIR << i << "/events/cas_count_write";

        type_file.open(type_path.str().c_str());
        read_event_file.open(read_event_path.str().c_str());
        write_event_file.open(write_event_path.str().c_str());

        if (!type_file.is_open() || !read_event_file.is_open() || !write_event_file.is_open())
            continue;

        // The type file is easy - just the decimal value
        type_file >> type;

        rd_config = read_perf_event(read_event_file);
        wr_config = read_perf_event(write_event_file);

        types.push_back(type);
        rd_configs.push_back(rd_config);
        wr_configs.push_back(wr_config);

        valid_uncore = i;
    }

    // Get what "cpu" values correspond to different NUMA nodes
    if (valid_uncore != -1) {
        std::stringstream cpumask_path;
        std::ifstream cpumask_file;
        std::string cpumask_str;
        size_t pos = 0;

        cpumask_path << BASE_DIR << valid_uncore << "/cpumask";

        cpumask_file.open(cpumask_path.str().c_str());

        if (!cpumask_file.is_open())
            return;

        cpumask_file >> cpumask_str;

        // The format of cpumask is CPU indices delimited by ","
        while (pos != std::string::npos) {
            pos = cpumask_str.find(",");
            std::string token = cpumask_str.substr(0, pos);
            int cpu = std::stoi(token);
            cpus.push_back(cpu);
            // + 1 for the length of the delimiter
            cpumask_str.erase(0, pos + 1);
            std::cout << cpu << std::endl;
        }
    }
}

void open_perf_events(int cpu, std::vector<uint32_t> types,
    std::vector<uint64_t> &configs, std::vector<int> &fds)
{
    int fd;
    struct perf_event_attr pe;

    assert(types.size() == configs.size());

    for (unsigned long i = 0; i < types.size(); i++) {
        memset(&pe, 0, sizeof(pe));
        pe.type = types[i];
        pe.size = sizeof(pe);
        pe.config = configs[i];
        pe.disabled = 1;
        pe.inherit = 1;

        fd = perf_event_open(&pe, -1, cpu, -1, 0);
        if (fd == -1) {
            std::cerr << "Error opening type: " << pe.type << " config: " <<
                std::hex << pe.config << std::dec << std::endl;
            return;
        }

        fds.push_back(fd);
    }
}

/*
 * Setup the perf event in sampling mode, mmap it, and return the header page
 * @pid: The pid to track
 * @type: The perf attribute type. See the perf_event_open man page.
 * @config: The perf attribute config. See the perf_event_open man page.
 * @config1: The perf attribute config1. See the perf_event_open man page.
 * @sample_period: The period at which to sample
 * @out_fd: Output - the file descriptor for the perf event
 */
struct perf_event_mmap_page *perf_sample_setup(pid_t pid, int cpu, uint64_t type, uint64_t config,
            uint64_t config1, uint64_t sample_period, int *out_fd)
{
    // Has to be 1 + 2^b pages
    constexpr uint64_t PERF_PAGES = (1 + (1 << 16));
    struct perf_event_attr attr;
    struct perf_event_mmap_page *p;
    int fd;

    memset(&attr, 0, sizeof(perf_event_attr));

    attr.type = type;
    attr.size = sizeof(perf_event_attr);
    attr.config = config;
    attr.config1 = config1;
    attr.sample_period = sample_period;
    attr.sample_type = PERF_SAMPLE_TID | PERF_SAMPLE_ADDR | PERF_SAMPLE_PHYS_ADDR;
    attr.pinned = 1;
    attr.disabled = 0;
    attr.exclude_kernel = 1;
    attr.exclude_hv = 1;
    attr.exclude_callchain_kernel = 1;
    attr.exclude_callchain_user = 1;
    attr.precise_ip = 1;

    fd = perf_event_open(&attr, pid, cpu, -1, 0);
    if (fd == -1) {
        std::cout << "Failed perf_event_open " << errno << std::endl;
        return NULL;
    }

    size_t mmap_size = sysconf(_SC_PAGESIZE) * PERF_PAGES;
    p = (struct perf_event_mmap_page*)mmap(NULL, mmap_size, PROT_READ | PROT_WRITE,
        MAP_SHARED, fd, 0);
    if (p == MAP_FAILED) {
        std::cout << "Failed to mmap perf_event_mmap_page" << std::endl;
        return NULL;
    }

    if (out_fd)
        *out_fd = fd;

    return p;
}

void apply_ioctl(int cmd, std::vector<int> fds)
{
    for (int fd : fds)
        ioctl(fd, cmd, 0);
}

