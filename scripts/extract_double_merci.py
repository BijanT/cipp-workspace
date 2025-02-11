#!/usr/bin/env python3

import sys
import os
import json
import re

from helpers import eprint,get_bandwidth

def get_avg_merci_times(merci_file):
    times = []
    f = open(merci_file, "r")
    for line in f:
        split = line.split(":")
        # Format: "Average Time: <time> ms"
        if split[0] == "Average Time":
            split = split[1].strip().split(" ")
            times.append(int(float(split[0])))

    return times

json_data = None
for line in sys.stdin:
    json_data = json.loads(line)

filename_stub = json_data['results_path']
cmd = json_data['cmd']
jid = json_data['jid']

merci_file = filename_stub + "merci"
bwmon_file = filename_stub + "bwmon"

if "--colloid" in cmd:
    strategy = "Colloid"
elif "--bwmfs" in cmd:
    strategy = "BandwidthMFS"
    ratios = re.findall("--bwmfs (\d+):\d+", cmd)
elif "--numactl" in cmd:
    strategy = "Numactl"
    ratios = re.findall("--numactl (\d+):\d+", cmd)
elif "--cipp" in cmd:
    strategy = "CIPP"
elif "--tpp" in cmd:
    strategy = "TPP"
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
local_bw = 0
remote_bw = 0
if use_bwmon:
    max_bandwidth, local_bw, remote_bw = get_bandwidth(bwmon_file, 50)

merci_times = get_avg_merci_times(merci_file)

outdata = {
    "JID": jid,
    "Command": cmd,
    "File": filename_stub,
    "Strategy": strategy,
    "Throttle": throttle,
    "Merci 1 (ms)": str(merci_times[0]),
    "Merci 2 (ms)": str(merci_times[1]),
    "Local BW (MB/s)": str(local_bw),
    "Remote BW (MB/s)": str(remote_bw),
    "Bandwidth (MB/s)": str(max_bandwidth),
}

if strategy == "BandwidthMFS" or strategy == "Numactl":
    outdata["Local Ratio"] = ratios[0]

print(json.dumps(outdata))
