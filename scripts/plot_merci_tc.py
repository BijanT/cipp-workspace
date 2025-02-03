#!/usr/bin/env python3

import sys
import csv
import matplotlib.pyplot as plt
import numpy as np

tc_ratio = "TC Ratio"
merci_ratio = "MERCI Ratio"
tc_time = "TC Time"
merci_time = "MERCI Time"
time_geomean = "Geomean"
bandwidth = "Bandwidth"
local_bw = "Local BW"
remote_bw = "Remote BW"
data_cols = [tc_ratio, merci_ratio, tc_time, merci_time,
            bandwidth, local_bw, remote_bw]

def build_data_grid(data, unique_tc_ratios, unique_merci_ratios, column, norm=None):
    num_tc_ratios = len(unique_tc_ratios)
    num_merci_ratios = len(unique_merci_ratios)
    num_data_points = len(data[column])
    grid = np.zeros((num_tc_ratios, num_merci_ratios))

    if norm is None:
        norm = min(data[column])

    for i in range(num_data_points):
        # Find the point in the grid to put this data point
        tc_ind = unique_tc_ratios.index(data[tc_ratio][i])
        merci_ind = unique_merci_ratios.index(data[merci_ratio][i])

        grid[tc_ind][merci_ind] = "{:.2f}".format(data[column][i] / norm)

    return grid

filename = sys.argv[1]
outfile = None
if len(sys.argv) >= 3:
    outfile = sys.argv[2]

data = {
    tc_ratio: [],
    merci_ratio: [],
    tc_time: [],
    merci_time: [],
    bandwidth: [],
    local_bw: [],
    remote_bw: [],
}

with open(filename, "r") as csvfile:
    reader = csv.DictReader(csvfile)
    for row in reader:
        for col in data_cols:
            data[col].append(int(float(row[col])))

# Get the unique ratios used for both tc and merci
# In all likely use cases, they will be the same, but generality is good I guess
unique_tc_ratios = sorted(set(data[tc_ratio]))
unique_merci_ratios = sorted(set(data[merci_ratio]))

grids = {}
# Build the simple grids that are normalized to themselves
for col in [tc_time, merci_time, bandwidth]:
    grids[col] = build_data_grid(data, unique_tc_ratios, unique_merci_ratios, col)

# The local and remote bandwidth plots are normalized to the min of total bw
min_tot_bw = min(data[bandwidth])
for col in [local_bw, remote_bw]:
    grids[col] = build_data_grid(data, unique_tc_ratios, unique_merci_ratios, col, min_tot_bw)

# The time sum plot is the geomean of the TC and MERCI normalized runtimes
grids[time_geomean] = np.zeros((len(unique_tc_ratios), len(unique_merci_ratios)))
for i in range(len(unique_tc_ratios)):
    for j in range(len(unique_merci_ratios)):
        geo = np.sqrt(grids[tc_time][i][j] * grids[merci_time][i][j])
        grids[time_geomean][i][j] = "{:.2f}".format(geo)

# The data points we want to plot
plotted_cols = [tc_time, merci_time, time_geomean, bandwidth, local_bw, remote_bw]
plot_titles = ["Normalized TC Runtime", "Normalized ER Runtime", "Normalized Runtime Geomean",
               "Normalized Bandwidth Utilization", "Normalized Local Bandwidth", "Normalized Remote Bandwidth"]

plt.figure(figsize=(192, 100))
fig, ax = plt.subplots(2, 3)
ax = ax.flatten()

for (i, col) in enumerate(plotted_cols):
    grid = grids[col]
    im = ax[i].imshow(grid, origin="lower")
    ax[i].set_title(plot_titles[i], fontsize=16)

    # Set the axis ticks to be the interleave ratios
    ax[i].set_xticks(np.arange(len(unique_merci_ratios)), labels=unique_merci_ratios, fontsize=12)
    ax[i].set_yticks(np.arange(len(unique_tc_ratios)), labels=unique_tc_ratios, fontsize=12)
    ax[i].set_xlabel("Percent of ER data in local memory", fontsize=14)
    ax[i].set_ylabel("Percent of TC data in local memory", fontsize=14)

    # Label each cell with the values
    for j in range(len(unique_tc_ratios)):
        for k in range(len(unique_merci_ratios)):
            ax[i].text(k, j, grid[j, k], ha="center", va="center", color="w",
                fontsize=12, weight="bold")

if outfile is not None:
    plt.savefig(outfile)
else:
    plt.show()
