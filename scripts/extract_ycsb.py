#!/usr/bin/env python3

import sys
import os
import json
import re

from helpers import eprint

def get_ycsb_throughput(ycsb_file):
    tput_pattern = re.compile(".*Throughput.* (\d+)\.\d+")

    f = open(ycsb_file, "r")
    for line in f:
        m = tput_pattern.match(line)
        if m is None:
            continue


        sample_tput = int(m.group(1))
        return sample_tput

    return None

json_data = None
for line in sys.stdin:
    json_data = json.loads(line)

filename_sub = json_data['results_path']
cmd = json_data['cmd']
jid = json_data['jid']

ycsb_file = filename_sub + "ycsb"

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

throughput = get_ycsb_throughput(ycsb_file)

outdata = {
    "JID": jid,
    "Command": cmd,
    "Strategy": strategy,
    "Throttle": throttle,
    "Throughput": str(throughput),
}

print(json.dumps(outdata))
