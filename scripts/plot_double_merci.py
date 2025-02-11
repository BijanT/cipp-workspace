#!/usr/bin/env python3

import sys
import os
import re
import matplotlib.pyplot as plt
import numpy as np

from helpers import eprint

def read_merci(merci_file, second_start_time):
    runtimes = [[], []]
    times = [[], []]
    merci_pattern = re.compile("REPEAT # (\d+).*: (\d+).* ms")

    f = open(merci_file, "r")
    for line in f:
        matches = merci_pattern.findall(line)

        if len(matches) != 1:
            continue

        # Convert from ms to seconds
        iteration_num = int(matches[0][0])
        runtime = int(matches[0][1]) / 1000.0

        # Does this entry belong to the first or second instance of merci?
        if iteration_num == len(runtimes[0]):
            inst = 0
        else:
            inst = 1

        if len(times[inst]) == 0 and inst == 0:
            times[inst].append(0)
        elif len(times[inst]) == 0 and inst == 1:
            times[inst].append(second_start_time)
        else:
            times[inst].append(times[inst][-1] + runtimes[inst][-1])
        runtimes[inst].append(runtime)

    return (runtimes, times)

merci_filename = sys.argv[1]
second_start_time = int(sys.argv[2])
outfile = None
if len(sys.argv) >= 4:
    outfile = sys.argv[3]

runtimes, times = read_merci(merci_filename, second_start_time)

plt.plot(times[0], runtimes[0], label="First")
plt.plot(times[1], runtimes[1], label="Second")

plt.legend()
plt.xlabel("Time (s)", fontsize=14)
plt.ylabel("Iteration Time (s)", fontsize=14)

if outfile is not None:
    plt.savefig(outfile)
else:
    plt.show()
