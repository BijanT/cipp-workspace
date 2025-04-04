#!/usr/bin/env python3

import sys
import csv
import copy
import matplotlib.pyplot as plt
import numpy as np

filename = sys.argv[1]
outfile = None
if len(sys.argv) > 2:
    outfile = sys.argv[2]

exp_set_data = {
    "Workload": [],
    "Colloid": [],
    "DMI": [],
}

exp_sets = ["120", "90", "60", "30", "lat"]
exp_data = {}
for s in exp_sets:
    exp_data[s] = copy.deepcopy(exp_set_data)

bar_colors = {
    "Colloid": "tab:blue",
    "DMI": "tab:orange",
}
bar_hatches = {
    "Colloid": "",
    "DMI": "/",
}

# Read the data
with open(filename, "r") as csvfile:
    reader = csv.DictReader(csvfile)
    for row in reader:
        exp_type = row['Exp Type']
        exp_data[exp_type]["Workload"].append(row['Workload'])
        exp_data[exp_type]['Colloid'].append(float(row['Colloid']))
        exp_data[exp_type]['DMI'].append(float(row['DMI']))

width = 0.33
x_tick_locs = []
x_tick_labels = []
type_boundaries = []
offset = 0
for exp_type in exp_sets:
    x_tick_locs = x_tick_locs + (np.arange(len(exp_data[exp_type]["Workload"])) + offset).tolist()
    # Subtract by width / 4 to center the bars in the boundaries
    type_boundaries.append(x_tick_locs[-1] + 1 - (width / 4))
    offset = x_tick_locs[-1] + 1.5
    for wkld in exp_data[exp_type]["Workload"]:
        x_tick_labels.append(wkld)
x_tick_locs = np.array(x_tick_locs)

plt.figure(figsize=(10, 6))
for i, exp_type in enumerate(exp_sets):
    start_loc = type_boundaries[i] - type_boundaries[0]
    loc = start_loc + np.arange(len(exp_data[exp_type]["Workload"]))
    multiplier = 0
    for attr, measurements in exp_data[exp_type].items():
        if attr == "Workload":
            continue
        offset = width * multiplier
        plt.bar(loc + offset, measurements, width, label=attr if i == 0 else "",
            color=bar_colors[attr], hatch=bar_hatches[attr])
        multiplier += 1

# Put separators between each experiment type
# We don't have to include the last one because there's nothing after it
for loc in type_boundaries[:-1]:
    plt.axvline(x=loc, linestyle="dotted", color='gray')

plt.legend(loc="upper right", fontsize=18)
plt.axhline(y=1, linestyle="--", color='black')
plt.title("Performance Comparison Between Colloid and DMI", fontsize=24)
plt.xticks(x_tick_locs + (0.5 * width), x_tick_labels, fontsize=16, rotation=-30)
plt.yticks(fontsize=18)
plt.ylabel("Relative Performance", fontsize=22)
plt.xlim(xmin=-2*width, xmax=type_boundaries[-1])

# Put in the labels for each section
secondary_x_labels = [
    "\n\n\n120 Cores",
    "\n\n\n90 Cores",
    "\n\n\n60 Cores",
    "\n\n\n30 Cores",
    "\n\n\nLatency Sensitive"
]
secondary_x_locs = []
for i in range(len(type_boundaries)):
    if i == 0:
        secondary_x_locs.append(type_boundaries[i] / 2)
    else:
        secondary_x_locs.append((type_boundaries[i] + type_boundaries[i - 1]) / 2)
ax = plt.gca()
sec = ax.secondary_xaxis(location=0)
sec.set_xticks(secondary_x_locs, secondary_x_labels, fontsize=18)
sec.tick_params(axis="x", which="both", length=0)

if outfile is not None:
    plt.savefig(outfile, bbox_inches="tight")
else:
    plt.show()
