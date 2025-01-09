#!/usr/bin/env python3

import sys
import os
import json
import re

from helpers import eprint,get_bandwidth

def get_avg_merci_time(merci_file):
    f = open(merci_file, "r")
    for line in f:
        split = line.split(":")
        # Format: "Average Time: <time> ms"
        if split[0] == "Average Time":
            split = split[1].strip().split(" ")
            return int(float(split[0]))

    return -1

def get_min_merci_time(merci_file):
    f = open(merci_file, "r")
    minimum = 10000000

    for line in f:
        if not ("REPEAT" in line):
            continue

        split = line.split(":")
        split2 = split[1].strip().split(" ")
        val = int(float(split2[0]))

        if val < minimum:
            minimum = val

    return minimum

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
    bwmfs_ratios = re.findall("--bwmfs (\d+):\d+", cmd)
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

avg_merci_time = get_avg_merci_time(merci_file)
min_merci_time = get_min_merci_time(merci_file)

outdata = {
    "JID": jid,
    "Command": cmd,
    "File": filename_stub,
    "Strategy": strategy,
    "Throttle": throttle,
    "Avg Merci (ms)": str(avg_merci_time),
    "Min Merci (ms)": str(min_merci_time),
    "Local BW (MB/s)": str(local_bw),
    "Remote BW (MB/s)": str(remote_bw),
    "Bandwidth (MB/s)": str(max_bandwidth),
}

if strategy == "BandwidthMFS":
    outdata["Local Ratio"] = bwmfs_ratios[0]

print(json.dumps(outdata))
