#!/usr/bin/env python3

import sys
import os
import re
import matplotlib.pyplot as plt
import numpy as np

from helpers import eprint

def read_merci(merci_file):
    runtimes = []
    times = []
    merci_pattern = re.compile("REPEAT.*: (\d+).* ms")

    f = open(merci_file, "r")
    for line in f:
        matches = merci_pattern.findall(line)

        if len(matches) != 1:
            continue

        # Convert from ms to seconds
        runtime = int(matches[0]) / 1000.0

        if len(times) == 0:
            times.append(0)
        else:
            times.append(times[-1] + runtimes[-1])
        runtimes.append(runtime)

    return (runtimes, times)

colloid_filename = sys.argv[1]
dmi_filename = sys.argv[2]
min_runtime = float(sys.argv[3])
outfile = None
if len(sys.argv) >= 5:
    outfile = sys.argv[4]

colloid_runtimes, colloid_times = read_merci(colloid_filename)
dmi_runtimes, dmi_times = read_merci(dmi_filename)

if len(colloid_runtimes) != len(dmi_runtimes):
    eprint("Number of iterations do not match")
    sys.exit(1)

max_time = max(colloid_times[-1], dmi_times[-1])
min_runtimes = [min_runtime for _ in range(len(colloid_runtimes))]
min_times = np.arange(0, max_time, max_time / len(min_runtimes)).tolist()

plt.plot(colloid_runtimes, label="Colloid")
plt.plot(dmi_runtimes, label="DMI")
plt.plot(min_runtimes, label="Optimal")

plt.legend()
plt.xlabel("Iteration #", fontsize=14)
plt.ylabel("Runtime (s)", fontsize=14)
plt.title("Embedding Reduction Runtime Across Iterations", fontsize=16)

if outfile is not None:
    plt.savefig(outfile)
else:
    plt.show()
