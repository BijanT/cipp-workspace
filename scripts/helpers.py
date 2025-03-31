import sys
import re

def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)

def get_bandwidth(bwmon_file, percentile=95):
    local_bws = []
    remote_bws = []
    total_bws = []
    node_bw_pattern = re.compile(r"Total (\d+) MB/s")

    if percentile < 0 or percentile > 100:
        eprint("Invalid percentile " + str(percentile))
        sys.exit()

    f = open(bwmon_file, "r")
    while True:
        local_str = f.readline()
        remote_str = f.readline()
        total_str = f.readline()

        if len(local_str) == 0:
            break;

        local_bw = int(node_bw_pattern.findall(local_str)[0])
        remote_bw = int(node_bw_pattern.findall(remote_str)[0])
        total_bw = int(total_str.split(":")[1])

        local_bws.append(local_bw)
        remote_bws.append(remote_bw)
        total_bws.append(total_bw)

        # Read past the line break between entries
        f.readline()

    sorted_bws = sorted(total_bws)
    percentile_ind = int(len(sorted_bws) * percentile / 100) - 1;
    i = total_bws.index(sorted_bws[percentile_ind])

    return (total_bws[i], local_bws[i], remote_bws[i])

def get_clover_runtime(clover_file):
    time_pattern = re.compile(r"Wall clock (\d+\.\d+)")
    f = open(clover_file, "r")
    runtime = 0

    # The file has multiple iterations, and it prints the time
    # since the beginning after each one. We only want the last
    for line in f:
        m = time_pattern.match(line.strip())
        if m is None:
            continue

        runtime = float(m.group(1))

    return runtime

def get_avg_gapbs_time(gapbs_file):
    f = open(gapbs_file, "r")
    for line in f:
        split = line.split(":")
        # Format: "Average Time: <time>"
        if split[0] == "Average Time":
            time = split[1].strip()
            return round(float(time), 2)

    return -1

def get_stream_triad(stream_file):
    triad_pattern = re.compile(r"Triad:\s+(\d+).*")
    f = open(stream_file, "r")

    for line in f:
        m = triad_pattern.match(line)
        if m is None:
            continue

        return float(m.group(1))

    return 0

def get_spec_time(spec_file):
    spec_pattern = re.compile(r".*; (\d+) total seconds elapsed")
    f = open(spec_file, "r")

    for line in f:
        m = spec_pattern.match(line)
        if m is None:
            continue

        return float(m.group(1))

    return 0

def moving_avg(data, window_size):
    moving_avgs = []
    for i in range(len(data) - window_size):
        window = data[i:i+window_size]
        avg = sum(window) / window_size
        moving_avgs.append(avg)

    return moving_avgs

def window_max(data, window_size):
    maxes = []
    for i in range(len(data) - window_size):
        window = data[i:i+window_size]
        maxes.append(max(window))

    return maxes

