#!/usr/bin/env python3

import sys
import csv
import matplotlib.pyplot as plt
import numpy as np

filename = sys.argv[1]
outfile = None
if len(sys.argv) >= 3:
    outfile = sys.argv[2]

ratios = []
total_bws = []
local_bws = []
remote_bws = []
with open(filename, "r") as csvfile:
    reader = csv.DictReader(csvfile)
    for row in reader:
        ratios.append(int(row["DRAM Ratio"]))
        total_bws.append(int(row["Total BW"]) / 1000)
        local_bws.append(int(row["Local BW"]) / 1000)
        remote_bws.append(int(row["Remote BW"]) / 1000)

plt.figure(figsize=(10,6))
plt.plot(ratios, local_bws, label="Local", linewidth=2.0)
plt.plot(ratios, remote_bws, label="Remote", linewidth=2.0)
plt.plot(ratios, total_bws, label="Total", linewidth=2.0)

plt.legend(fontsize=18)
plt.xticks(fontsize=18)
plt.yticks(fontsize=18)
plt.xlabel("Percent of Data in Local Memory", fontsize=18)
plt.ylabel("Bandwidth (GB/s)", fontsize=18)
plt.title("Peak Bandwidth Utilization By Interleave Ratio", fontsize=22)

if outfile is not None:
    plt.savefig(outfile)
else:
    plt.show()


