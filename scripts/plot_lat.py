#!/usr/bin/env python3

import sys
import os
import re
import matplotlib.pyplot as plt

from helpers import moving_avg,eprint

def read_lat(latency_file):
    local_lats = []
    remote_lats = []
    lat_pattern = re.compile(r"local (\d+) remote (\d+)")

    f = open(latency_file, "r")
    for line in f:
        matches = lat_pattern.findall(line.lower())

        if len(matches) != 1:
            eprint("Invalid line " + line)
            sys.exit(1)

        (local, remote) = matches[0]
        local = int(local)
        remote = int(remote)

        local_lats.append(local)
        remote_lats.append(remote)

    return (local_lats, remote_lats)

filename = sys.argv[1]
workload = sys.argv[2]
outfile = None
if len(sys.argv) >= 4:
    outfile = sys.argv[3]

(local_lat, remote_lat) = read_lat(filename)
local_lat = moving_avg(local_lat, 5)
remote_lat = moving_avg(remote_lat, 5)
# Bandwidth measurements are collected once every 5s
time_s = [0.1 * i for i in range(len(local_lat))]

plt.figure(figsize=(10,6))

plt.plot(time_s, local_lat, label="Local", linewidth=2.0)
plt.plot(time_s, remote_lat, label="Remote", linewidth=2.0)
plt.ylim(0, 1500)

#plt.legend(fontsize=18, ncols=2, bbox_to_anchor=(1.25, 1.25))
plt.xticks(fontsize=18)
plt.yticks(fontsize=18)
plt.xlabel("Time (s)", fontsize=22)
plt.ylabel("Access Latency (cycles)", fontsize=22)
#plt.title("Access Latency of " + workload, fontsize=24)

if outfile is not None:
    plt.savefig(outfile, bbox_inches="tight")
else:
    plt.show()
