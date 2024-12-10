#!/usr/bin/env python3

import sys
import os
import json
import re

def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)

def get_max_bandwidth(bwmon_file):
    max_bandwidth = 0
    max_local_bw = 0
    max_remote_bw = 0
    node_bw_pattern = re.compile("Total (\d+) MB/s")

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

        if total_bw > max_bandwidth:
            max_bandwidth = total_bw
            max_local_bandwidth = local_bw
            max_remote_bandwidth = remote_bw

        # Read past the line break between entries
        f.readline()

    return (max_bandwidth, max_local_bandwidth, max_remote_bandwidth)

def get_avg_merci_time(merci_file):
    time_sum = 0.0
    iters = 0
    sample_pattern = re.compile("REPEAT .* : (\d+\.\d\d)")
    skipping = True
    first_sample = 0

    f = open(merci_file, "r")
    for line in f:
        m = sample_pattern.match(line)
        if m is None:
            continue

        sample_time = float(m.group(1))

        # The first few samples of MERCI are lower than the rest when tc
        # is starting up, so skip them
        if skipping:
            if first_sample == 0:
                first_sample = sample_time
                continue

            if (sample_time - first_sample) / first_sample > 0.05:
                skipping = False
            else:
                continue

        time_sum += sample_time
        iters += 1

    if iters == 0:
        return -1
    else:
        return int(time_sum / iters)

def get_avg_tc_time(tc_file):
    f = open(tc_file, "r")
    for line in f:
        split = line.split(":")
        if split[0] == "Average Time":
            return int(float((split[1].strip())))

    return -1

json_data = None
for line in sys.stdin:
    json_data = json.loads(line)

filename_stub = json_data['results_path']
cmd = json_data['cmd']
jid = json_data['jid']

merci_file = filename_stub + "merci"
tc_file = filename_stub + "gapbs"
bwmon_file = filename_stub + "bwmon"

if "--colloid" in cmd:
    strategy = "Colloid"
elif "--bwmfs" in cmd:
    strategy = "BandwidthMFS"
    bwmfs_ratios = re.findall("--bwmfs (\d+):\d+", cmd)
elif "--cipp" in cmd:
    strategy = "CIPP"
else:
    strategy = "None"

if "--quartz" in cmd:
    throttle = "Quartz"
elif "--msr_throttle" in cmd:
    throttle = "MSR"
else:
    throttle = "None"

use_bwmon = "--bwmon" in cmd

max_bandwidth = 0
if use_bwmon:
    max_bandwidth, _, _ = get_max_bandwidth(bwmon_file)

merci_time = get_avg_merci_time(merci_file)
tc_time = get_avg_tc_time(tc_file)

outdata = {
    "JID": jid,
    "Command": cmd,
    "File": filename_stub,
    "Strategy": strategy,
    "Throttle": throttle,
    "Merci (ms)": str(merci_time),
    "GAPBS TC (s)": str(tc_time),
    "Max Bandwidth (MB/s)": str(max_bandwidth),
}

if strategy == "BandwidthMFS":
    outdata["TC Local Ratio"] = bwmfs_ratios[0]
    outdata["MERCI Local Ratio"] = bwmfs_ratios[1]

print(json.dumps(outdata))
