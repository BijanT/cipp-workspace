#!/usr/bin/env python3

import sys
import csv
import matplotlib.pyplot as plt
import numpy as np

tc_ratio = "TC Ratio"
merci_ratio = "MERCI Ratio"
tc_time = "TC Time"
merci_time = "MERCI Time"
bandwidth = "Bandwidth"
data_cols = [tc_ratio, merci_ratio, tc_time, merci_time, bandwidth]

def build_data_grid(data, unique_tc_ratios, unique_merci_ratios, column):
    num_tc_ratios = len(unique_tc_ratios)
    num_merci_ratios = len(unique_merci_ratios)
    num_data_points = len(data[column])
    grid = np.zeros((num_tc_ratios, num_merci_ratios))

    min_val = min(data[column])

    for i in range(num_data_points):
        # Find the point in the grid to put this data point
        tc_ind = unique_tc_ratios.index(data[tc_ratio][i])
        merci_ind = unique_merci_ratios.index(data[merci_ratio][i])

        grid[tc_ind][merci_ind] = "{:.2f}".format(data[column][i] / min_val)

    return grid

filename = sys.argv[1]

data = {
    tc_ratio: [],
    merci_ratio: [],
    tc_time: [],
    merci_time: [],
    bandwidth: [],
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

# The data points we want to plot
plotted_cols = [tc_time, merci_time, bandwidth]
plot_titles = ["Normalized TC Runtime", "Normalized MERCI Runtime", "Normalized Bandwidth Utilization"]

fig, ax = plt.subplots(1, len(plotted_cols))

for (i, col) in enumerate(plotted_cols):
    grid = build_data_grid(data, unique_tc_ratios, unique_merci_ratios, col)
    im = ax[i].imshow(grid, origin="lower")
    ax[i].set_title(plot_titles[i])

    # Set the axis ticks to be the interleave ratios
    ax[i].set_xticks(np.arange(len(unique_merci_ratios)), labels=unique_merci_ratios)
    ax[i].set_yticks(np.arange(len(unique_tc_ratios)), labels=unique_tc_ratios)
    ax[i].set_xlabel("Percent of MERCI data in local memory")
    ax[i].set_ylabel("Percent of TC data in local memory")

    # Label each cell with the values
    for j in range(len(unique_tc_ratios)):
        for k in range(len(unique_merci_ratios)):
            ax[i].text(k, j, grid[j, k], ha="center", va="center", color="w")

plt.show()
