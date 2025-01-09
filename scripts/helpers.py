import sys
import re

def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)

def get_bandwidth(bwmon_file, percentile=95):
    local_bws = []
    remote_bws = []
    total_bws = []
    node_bw_pattern = re.compile("Total (\d+) MB/s")

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

