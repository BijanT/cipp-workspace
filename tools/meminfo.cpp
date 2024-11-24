#include <iostream>
#include <fstream>
#include <sstream>
#include <string>

#include <cstdlib>
#include <stdio.h>
#include <unistd.h>

constexpr int PAGE_SIZE = 4096;
constexpr int HPAGE_SIZE = (512 * PAGE_SIZE);
constexpr int PAGE_SHIFT = 12;

constexpr int PME_SIZE = 8;
constexpr uint64_t PM_PRESENT = ((uint64_t)1 << 63);
constexpr uint64_t PM_SWAPPED = ((uint64_t)1 << 62);
constexpr uint64_t PM_FILEPAGE = ((uint64_t)1 << 61);
constexpr uint64_t PM_EXCLUSIVE = ((uint64_t)1 << 56);
constexpr uint64_t PM_SOFTDIRTY = ((uint64_t)1 << 55);
constexpr uint64_t PM_PFN_MASK (((uint64_t)1 << 54) - 1);
constexpr uint64_t KPF_ANON = ((uint64_t)1 << 12);
constexpr uint64_t KPF_THP = ((uint64_t)1 << 22);

uint64_t read_from_file(FILE *f)
{
    uint8_t c;
    uint64_t value = 0;

    for (int i = 0; i < PME_SIZE; i++) {
        c = fgetc(f);
        value += (((uint64_t)c) << (i*8));
    }

    return value;
}

int read_maps(pid_t pid, uint64_t local_start, uint64_t local_end)
{
    std::string line;
    std::stringstream maps_filename;
    std::stringstream pagemap_filename;
    std::stringstream kpf_filename;
    std::ifstream maps_file;
    uint64_t local_size = 0;
    uint64_t remote_size = 0;
    FILE *pagemap_file;
    FILE *kpf_file;

    maps_filename << "/proc/" << pid << "/maps";
    pagemap_filename << "/proc/" << pid << "/pagemap";
    kpf_filename << "/proc/kpageflags";

    maps_file.open(maps_filename.str());
    if (!maps_file.is_open()) {
        std::cout << "Unable to open " << maps_filename.str() << std::endl;
        return -1;
    }

    // CPP style file streams didn't seem to work, so fall back to C style
    pagemap_file = fopen(pagemap_filename.str().c_str(), "rb");
    if (!pagemap_file) {
        std::cout << "Unable to open " << pagemap_filename.str() << std::endl;
        return -1;
    }

    kpf_file = fopen(kpf_filename.str().c_str(), "rb");
    if (!kpf_file) {
        std::cout << "Unable to open " << kpf_filename.str() << std::endl;
        return -1;
    }

    while (std::getline(maps_file, line)) {
        std::stringstream maps_stream(line);
        std::string start_str;
        std::string end_str;
        uint64_t region_start;
        uint64_t region_end;
        uint64_t virt_pfn, phys_pfn;
        uint64_t pm_entry;
        uint64_t kpf_entry;
        uint64_t addr;
        bool is_local;
        int page_size;
        int status;

        std::getline(maps_stream, start_str, '-');
        region_start = std::stoul(start_str, nullptr, 16);
        std::getline(maps_stream, end_str, ' ');
        region_end = std::stoul(end_str, nullptr, 16);

        addr = region_start;
        while (addr < region_end) {
            virt_pfn = addr >> PAGE_SHIFT;

            status = fseek(pagemap_file, virt_pfn * PME_SIZE, SEEK_SET);
            if (status) {
                std::cout << "Failed to do fseek!" << std::endl;
                return -1;
            }

            pm_entry = read_from_file(pagemap_file);
            if (!(pm_entry & PM_PRESENT)) {
                addr += PAGE_SIZE;
                continue;
            }
            if (pm_entry & PM_SWAPPED) {
                addr += PAGE_SIZE;
                continue;
            }
            if (pm_entry & PM_FILEPAGE) {
                addr += PAGE_SIZE;
                continue;
            }
            if (!(pm_entry & PM_EXCLUSIVE)) {
                addr += PAGE_SIZE;
                continue;
            }

            phys_pfn = pm_entry & PM_PFN_MASK;
            is_local = local_start <= phys_pfn && phys_pfn < local_end;

            // Is the page THP?
            status = fseek(kpf_file, phys_pfn * PME_SIZE, SEEK_SET);
            if (status) {
                std::cout << "Failed to do fseek kpf!" << std::endl;
                return -1;
            }
            kpf_entry = read_from_file(kpf_file);
            page_size = (kpf_entry & KPF_THP) ? HPAGE_SIZE : PAGE_SIZE;

            if (is_local)
                local_size += page_size;
            else
                remote_size += page_size;

	    addr += page_size;
        }
    }

    // Convert the sizes to MB
    local_size = local_size >> 20;
    remote_size = remote_size >> 20;

    std::cout << "Local " << local_size << "MB, Remote " << remote_size << "MB, "
            << "Total " << local_size + remote_size << "MB" << std::endl;

    std::cout << (local_size * 100) / (local_size + remote_size) << "\% local"
            << std::endl;

    return 0;
}

int main(int argc, char *argv[])
{
    pid_t pid;
    uint64_t local_start;
    uint64_t local_end;

    if (argc != 4) {
        std::cout << "Usage: ./meminfo <pid> <local memory start> <local memory end>" << std::endl;
        return -1;
    }

    pid = std::stoi(argv[1]);
    local_start = std::stoul(argv[2], nullptr, 16) >> PAGE_SHIFT;
    local_end = std::stoul(argv[3], nullptr, 16) >> PAGE_SHIFT;

    return read_maps(pid, local_start, local_end);
}
