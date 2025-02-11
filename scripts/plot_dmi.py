#!/usr/bin/env python3

import sys
import os
import re
import matplotlib.pyplot as plt

from helpers import eprint

def read_dmi(dmi_file):
    # Start with application initially all local
    ratios = [100]
    dmi_pattern = re.compile("Target ratio: (\d+)")

    f = open(dmi_file, "r")
    for line in f:
        matches = dmi_pattern.findall(line)

        if len(matches) != 1:
            continue

        ratio = int(matches[0])

        ratios.append(ratio)

    return ratios

filename = sys.argv[1]
workload = sys.argv[2]
outfile = None
if len(sys.argv) >= 4:
    outfile = sys.argv[3]

ratios = read_dmi(filename)
# CIPP updates every 15 seconds
time_s = [8 * i for i in range(len(ratios))]

plt.plot(time_s, ratios)
plt.ylim(0, 105)

plt.xlabel("Time (s)", fontsize=14)
plt.ylabel("Percent of Data in Local Memory", fontsize=14)
plt.title("DMI Interleave Ratio of " + workload, fontsize=16)

if outfile is not None:
    plt.savefig(outfile)
else:
    plt.show()
