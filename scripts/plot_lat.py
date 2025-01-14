#!/usr/bin/env python3

import sys
import os
import re
import matplotlib.pyplot as plt

from helpers import eprint

def read_lat(latency_file):
    local_lats = []
    remote_lats = []
    lat_pattern = re.compile("local (\d+) remote (\d+)")

    f = open(latency_file, "r")
    for line in f:
        matches = lat_pattern.findall(line)

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
if len(sys.argv) >= 4:
    outfile = sys.argv[3]

(local_lat, remote_lat) = read_lat(filename)
# Bandwidth measurements are collected once every 5s
time_s = [5 * i for i in range(len(local_lat))]

plt.plot(time_s, local_lat, label="Local")
plt.plot(time_s, remote_lat, label="Remote")

plt.legend()
plt.xlabel("Time (s)", fontsize=14)
plt.ylabel("Access Latency (cycles)", fontsize=14)
plt.title("Access Latency of " + workload, fontsize=16)

if outfile is not None:
    plt.savefig(outfile)
else:
    plt.show()
