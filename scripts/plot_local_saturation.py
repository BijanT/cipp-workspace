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
with open(filename, "r") as csvfile:
    reader = csv.DictReader(csvfile)
    for row in reader:
        ratios.append(int(row["DRAM Ratio"]))
        total_bws.append(int(row["Total BW"]) / 1024)
        local_bws.append(int(row["Local BW"]) / 1024)

plt.plot(ratios, total_bws, label="Total")
plt.plot(ratios, local_bws, label="Local")

plt.legend()
plt.xlabel("Percent of Data in Local Memory", fontsize=14)
plt.ylabel("Bandwidth (GB/s)", fontsize=14)
plt.title("Peak Bandwidth Utilization By Interleave Ratio", fontsize=16)

if outfile is not None:
    plt.savefig(outfile)
else:
    plt.show()


