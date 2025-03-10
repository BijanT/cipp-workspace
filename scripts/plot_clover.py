#!/usr/bin/env python3

import sys
import os
import re
import matplotlib.pyplot as plt
import numpy as np

from helpers import eprint

def read_clover(clover_file):
    last_time = 0
    runtimes = []
    times = []
    time_pattern = re.compile("Wall clock (\d+\.\d+)")

    f = open(clover_file, "r")
    for line in f:
        m = time_pattern.findall(line)

        if len(m) != 1:
            continue

        # Convert from ms to seconds
        total_time = float(m[0])

        times.append(last_time)
        runtimes.append(total_time - last_time)
        last_time = total_time

    return (runtimes, times)

colloid_filename = sys.argv[1]
dmi_filename = sys.argv[2]
static_filename = sys.argv[3]
outfile = None
if len(sys.argv) >= 5:
    outfile = sys.argv[4]

colloid_runtimes, colloid_times = read_clover(colloid_filename)
dmi_runtimes, dmi_times = read_clover(dmi_filename)
static_runtimes, static_times = read_clover(static_filename)

plt.plot(colloid_times, colloid_runtimes, label="Colloid", linewidth=2.0)
plt.plot(dmi_times, dmi_runtimes, label="DMI", linewidth=2.0)
plt.plot(static_times, static_runtimes, label="Static", linewidth=2.0)

plt.ylim(ymin=0)

plt.legend(fontsize=14)
plt.xticks(fontsize=14)
plt.yticks(fontsize=14)
plt.xlabel("Time (s)", fontsize=16)
plt.ylabel("Iteration Time (s)", fontsize=16)
plt.title("CloverLeaf Iteration Speed Over Time", fontsize=18)

if outfile is not None:
    plt.savefig(outfile)
else:
    plt.show()
