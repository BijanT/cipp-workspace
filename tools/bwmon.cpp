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

void get_perf_info(std::vector<uint32_t> &types, std::vector<uint64_t> &rd_configs,
    std::vector<uint64_t> &wr_configs)
{
    const std::string BASE_DIR = "/sys/devices/uncore_imc_";
    uint32_t type;
    uint64_t rd_config;
    uint64_t wr_config;

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
    }
}

void open_perf_events(std::vector<uint32_t> types, std::vector<uint64_t> &configs, std::vector<int> &fds)
{
    int fd;
    struct perf_event_attr pe;

    assert(types.size() == configs.size());

    for (int i = 0; i < types.size(); i++) {
        memset(&pe, 0, sizeof(pe));
        pe.type = types[i];
        pe.size = sizeof(pe);
        pe.config = configs[i];
        pe.disabled = 1;
        pe.inherit = 1;

        // What to put in the cpu to read from each socket can be found by reading
        // /sys/devices/uncore_imc_0/cpumask - we only care about socket 0, which
        // is represented by CPU 0
        fd = perf_event_open(&pe, -1, 0, -1, 0);
        if (fd == -1) {
            std::cerr << "Error opening type: " << pe.type << " config: " <<
                std::hex << pe.config << std::dec << std::endl;
            return;
        }

        fds.push_back(fd);
    }
}

void apply_ioctl(int cmd, std::vector<int> fds)
{
    for (int fd : fds)
        ioctl(fd, cmd, 0);
}

int main(int argc, char* argv[])
{
    std::vector<uint32_t> types;
    std::vector<uint64_t> rd_configs;
    std::vector<uint64_t> wr_configs;
    std::vector<int> rd_fds;
    std::vector<int> wr_fds;
    std::ofstream out_file;
    int sample_interval_ms;
    uint64_t rd_count, wr_count;
    uint64_t rd_bw, wr_bw;
    uint64_t count;
    pid_t pid;

    if (argc < 4) {
        std::cerr << "Usage: ./bwmon <out file> <Sample Interval (ms)> <cmd> <args>" << std::endl;
        return -1;
    }

    out_file.open(argv[1]);
    if (!out_file.is_open()) {
        std::cerr << "Could not open " << argv[1] << " for writting" << std::endl;
        return -1;
    }

    sample_interval_ms = atoi(argv[2]);
    if (!sample_interval_ms) {
        std::cout << "Invalid sample interval: " << argv[2] << std::endl;
        return -1;
    }

    get_perf_info(types, rd_configs, wr_configs);

    open_perf_events(types, rd_configs, rd_fds);
    open_perf_events(types, wr_configs, wr_fds);

    pid = fork();
    if (pid == -1) {
        std::cerr << "Error forking proc: " << errno << std::endl;
    } else if (pid == 0) {
        execvp(argv[3], &argv[3]);
        std::cerr << "Error execing file!" << std::endl;
        return -1;
    }

    while (!waitpid(pid, nullptr, WNOHANG)) {
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

        out_file << "Read " << rd_bw << " Write " << wr_bw << " Total "
            << rd_bw + wr_bw << " MB/s" << std::endl;
    }

    return 0;
}

