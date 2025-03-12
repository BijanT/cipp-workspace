#!/usr/bin/env python3

import sys
import os
import re
import matplotlib.pyplot as plt

from helpers import eprint

def read_dmi(dmi_file):
    # Start with application initially all local
    ratios = [100]
    dmi_pattern = re.compile(r"Target ratio: (\d+)")

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
time_s = [9 * i for i in range(len(ratios))]

plt.plot(time_s, ratios, linewidth=2.0)
plt.ylim(0, 105)

plt.xlabel("Time (s)", fontsize=22)
plt.ylabel("% Data in Local Memory", fontsize=22)
plt.xticks(fontsize=18)
plt.yticks(fontsize=18)
plt.title("Interleave Ratio of " + workload, fontsize=24)

if outfile is not None:
    plt.savefig(outfile, bbox_inches="tight")
else:
    plt.show()
