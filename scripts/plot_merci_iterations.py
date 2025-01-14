#!/usr/bin/env python3

import sys
import os
import re
import matplotlib.pyplot as plt

from helpers import eprint

def read_merci(merci_file):
    runtimes = []
    merci_pattern = re.compile("REPEAT.*: (\d+).* ms")

    f = open(merci_file, "r")
    for line in f:
        matches = merci_pattern.findall(line)

        if len(matches) != 1:
            continue

        # Convert from ms to seconds
        runtime = int(matches[0]) / 1000.0

        runtimes.append(runtime)

    return runtimes

colloid_filename = sys.argv[1]
dmi_filename = sys.argv[2]
min_runtime = float(sys.argv[3])
if len(sys.argv) >= 5:
    outfile = sys.argv[4]

colloid_runtimes = read_merci(colloid_filename)
dmi_runtimes = read_merci(dmi_filename)

if len(colloid_runtimes) != len(dmi_runtimes):
    eprint("Number of iterations do not match")
    sys.exit(1)

min_runtimes = [min_runtime for _ in range(len(colloid_runtimes))]

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
