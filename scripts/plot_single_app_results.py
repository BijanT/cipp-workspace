#!/usr/bin/env python3

import sys
import csv
import matplotlib.pyplot as plt
import numpy as np

filename = sys.argv[1]
outfile = None
if len(sys.argv) > 2:
    outfile = sys.argv[2]

workloads = []
runtimes = {
    "Colloid": [],
    "DMI": [],
}

# Read the data
with open(filename, "r") as csvfile:
    reader = csv.DictReader(csvfile)
    for row in reader:
        workloads.append(row['Workload'])
        runtimes['Colloid'].append(float(row['Colloid']))
        runtimes['DMI'].append(float(row['DMI']))

x = np.arange(len(workloads))
print(x)
width = 0.33
multiplier = 0

plt.figure(figsize=(10, 6))
for attr, measurements in runtimes.items():
    offset = width * multiplier
    plt.bar(x + offset, measurements, width, label=attr)
    multiplier += 1

plt.legend(loc=(1.01, 0.8), fontsize=18)
plt.axhline(y=1, linestyle="--", color='black')
plt.title("Performance Comparison Between Colloid and DMI", fontsize=24)
plt.xticks(x + (0.5 * width), workloads, fontsize=18, rotation=0)
plt.yticks(fontsize=18)
plt.ylabel("Relative Performance", fontsize=22)

if outfile is not None:
    plt.savefig(outfile, bbox_inches="tight")
else:
    plt.show()
