#!/usr/bin/env python3

import sys
import os
import json
import re

from helpers import eprint,get_bandwidth,get_avg_gapbs_time

def get_min_gapbs_time(gapbs_file):
    f = open(gapbs_file, "r")
    minimum = 10000000

    for line in f:
        split = line.split(":")
        if split[0] == "Trial Time":
            time_str = split[1].strip()
            val = round(float(time_str), 2)

            if val < minimum:
                minimum = val

    return minimum

json_data = None
for line in sys.stdin:
    json_data = json.loads(line)

filename_stub = json_data['results_path']
cmd = json_data['cmd']
jid = json_data['jid']

gapbs_file = filename_stub + "gapbs"
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

avg_time = get_avg_gapbs_time(gapbs_file)
min_time = get_min_gapbs_time(gapbs_file)

outdata = {
    "JID": jid,
    "Command": cmd,
    "File": filename_stub,
    "Strategy": strategy,
    "Throttle": throttle,
    "Avg Time (s)": str(avg_time),
    "Min Time (s)": str(min_time),
    "Local BW (MB/s)": str(local_bw),
    "Remote BW (MB/s)": str(remote_bw),
    "Bandwidth (MB/s)": str(max_bandwidth),
}

if strategy == "BandwidthMFS" or strategy == "Numactl":
    outdata["Local Ratio"] = ratios[0]

print(json.dumps(outdata))
