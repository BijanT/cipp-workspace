#!/usr/bin/env python3

import sys
import os
import re
import matplotlib.pyplot as plt

from helpers import eprint

def add_record(bw_list, data_point):
    # We see big drops in BW between merci iterations, that makes the graph
    # hard to read. Smooth those over.
    if len(bw_list) != 0 and data_point <= bw_list[-1] / 4:
        bw_list.append(bw_list[-1])
    else:
        bw_list.append(data_point)

def read_bw(bwmon_file):
    local_bws = []
    remote_bws = []
    node_bw_pattern = re.compile("Node (\d): .* Total (\d+) MB/s")

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
            add_record(local_bws, bw)
        elif node == 1:
            add_record(remote_bws, bw)
        else:
            eprint("Invalid node " + str(node) + " from line " + line)
            sys.exit(1)

    if len(local_bws) != len(remote_bws):
        eprint("Different local and remote bandwidth measurements!")
        sys.exit(1)

    return (local_bws, remote_bws)

filename = sys.argv[1]
workload = sys.argv[2]
if len(sys.argv) >= 4:
    outfile = sys.argv[3]

(local_bw, remote_bw) = read_bw(filename)
# Bandwidth measurements are collected once every 200ms
time_s = [0.2 * i for i in range(len(local_bw))]

plt.plot(time_s, local_bw, label="Local")
plt.plot(time_s, remote_bw, label="Remote")

plt.legend()
plt.xlabel("Time (s)", fontsize=14)
plt.ylabel("Bandwidth Usage (GB/s)", fontsize=14)
plt.title("Bandwidth Utilization of " + workload, fontsize=16)

if outfile is not None:
    plt.savefig(outfile)
else:
    plt.show()
