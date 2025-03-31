#!/usr/bin/env python3

import sys
import os
import re
import matplotlib.pyplot as plt

from helpers import moving_avg,eprint

def read_bw(bwmon_file):
    local_bws = []
    remote_bws = []
    node_bw_pattern = re.compile(r"Node (\d): .* Total (\d+) MB/s")

    f = open(bwmon_file, "r")
    for line in f:
        matches = node_bw_pattern.findall(line)

        if len(matches) != 1:
            continue

        (node, bw) = matches[0]
        # Convert from MB/s to GB/s
        node = int(node)
        bw = float(bw) / 1024

        if node == 0:
            local_bws.append(bw)
        elif node == 1:
            remote_bws.append(bw)
        else:
            eprint("Invalid node " + str(node) + " from line " + line)
            sys.exit(1)

    if len(local_bws) != len(remote_bws) and len(remote_bws) != 0:
        eprint("Different local and remote bandwidth measurements!")
        sys.exit(1)

    return (local_bws, remote_bws)

filename = sys.argv[1]
workload = sys.argv[2]
outfile = None
if len(sys.argv) >= 4:
    outfile = sys.argv[3]

(local_bw, remote_bw) = read_bw(filename)
avg_local_bw = moving_avg(local_bw, 10)
avg_remote_bw = moving_avg(remote_bw, 10)
# Bandwidth measurements are collected once every 100ms
time_s = [0.1 * i for i in range(len(avg_local_bw))]

plt.plot(time_s, avg_local_bw, label="Local", linewidth=2.0)
if len(remote_bw) != 0:
    total_bw = [local + remote for local, remote in zip(avg_local_bw, avg_remote_bw)]
    plt.plot(time_s, avg_remote_bw, label="Remote", linewidth=2.0)
    plt.plot(time_s, total_bw, label="Total", linewidth=2.0)

plt.ylim(ymin=0, ymax=425)

plt.legend(fontsize=18)
plt.xticks(fontsize=18)
plt.yticks(fontsize=18)
plt.xlabel("Time (s)", fontsize=22)
plt.ylabel("Bandwidth Usage (GB/s)", fontsize=22)
plt.title("Bandwidth Utilization of " + workload, fontsize=24)

if outfile is not None:
    plt.savefig(outfile)
else:
    plt.show()
