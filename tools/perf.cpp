#include <cassert>
#include <cstring>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include <linux/perf_event.h>
#include <sys/ioctl.h>
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
        std::ifstream type_file;

        type_path << BASE_DIR << i << "/type";

        type_file.open(type_path.str().c_str());

        if (!type_file.is_open())
            continue;

        // The type file is easy - just the decimal value
        type_file >> type;

        types.push_back(type);

        valid_uncore = i;
    }

    // Get what "cpu" values correspond to different NUMA nodes
    // and the event codes for rd and write events
    if (valid_uncore != -1) {
        std::stringstream cpumask_path;
        std::stringstream read_event_path;
        std::stringstream write_event_path;
        std::ifstream cpumask_file;
        std::ifstream read_event_file;
        std::ifstream write_event_file;
        std::string cpumask_str;
        size_t pos = 0;

        cpumask_path << BASE_DIR << valid_uncore << "/cpumask";
        read_event_path << BASE_DIR << valid_uncore << "/events/cas_count_read";
        write_event_path << BASE_DIR << valid_uncore << "/events/cas_count_write";

        cpumask_file.open(cpumask_path.str().c_str());
        read_event_file.open(read_event_path.str().c_str());
        write_event_file.open(write_event_path.str().c_str());

        if (!cpumask_file.is_open() || !read_event_file.is_open() || !write_event_file.is_open())
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

        rd_config = read_perf_event(read_event_file);
        wr_config = read_perf_event(write_event_file);
        rd_configs.push_back(rd_config);
        wr_configs.push_back(wr_config);
#ifdef GNR
        // Quick hack: In SPR and GNR, there are two channels for reads and writes
        // SCH0 and SCH1. SCH0 is found in the file. The event for SCH1 is just
        // one larger than SCH0
        rd_configs.push_back(rd_config + 1);
        wr_configs.push_back(wr_config + 1);
#endif

    }
}

void open_perf_events(int cpu, std::vector<uint32_t> types,
    std::vector<uint64_t> &configs, std::vector<int> &fds)
{
    int fd;
    struct perf_event_attr pe;

    for (unsigned long i = 0; i < types.size(); i++) {
        memset(&pe, 0, sizeof(pe));
        pe.type = types[i];
        pe.size = sizeof(pe);
        pe.disabled = 1;
        pe.inherit = 1;

        for (unsigned long j = 0; j < configs.size(); j++) {
                pe.config = configs[j];
                fd = perf_event_open(&pe, -1, cpu, -1, 0);
                if (fd == -1) {
                    std::cerr << "Error opening type: " << pe.type << " config: " <<
                        std::hex << pe.config << std::dec << std::endl;
                    continue;
                }

                fds.push_back(fd);
        }
    }
}

void apply_ioctl(int cmd, std::vector<int> fds)
{
    for (int fd : fds)
        ioctl(fd, cmd, 0);
}

