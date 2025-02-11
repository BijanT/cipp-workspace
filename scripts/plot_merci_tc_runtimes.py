#!/usr/bin/env python3
import matplotlib.pyplot as plt
import numpy as np
import sys

outfile = None
if len(sys.argv) > 1:
    outfile = sys.argv[1]

configurations = ['DMI', 'Old DMI', 'Colloid', 'Tiering']

# Test names
tests = ['Triangle Counting', 'Embedding Reduction', 'Geomean']

# Example performance data (replace these with your actual performance data)
performance_data = {
    'Triangle Counting': [1.018, 1.023, 1.15, 1.35],  # performance values for A and B
    'Embedding Reduction': [1.16, 1.24, 1.42, 1.55],
    'Geomean': [1.089, 1.12, 1.27, 1.45]
}

# Create an array of indices for each test
x = np.arange(len(tests))

# Set the width of the bars
width = 0.20

# Create the bar chart
fig, ax = plt.subplots(figsize=(10, 6))

# Plot bars for each configuration
ax.bar(x - (1.5 * width), [performance_data[test][0] for test in tests], width, label='DMI', color='yellow')
ax.bar(x - (0.5 * width), [performance_data[test][1] for test in tests], width, label='Old DMI', color='blue')
ax.bar(x + (0.5 * width), [performance_data[test][2] for test in tests], width, label='Colloid', color='orange')
ax.bar(x + (1.5 * width), [performance_data[test][3] for test in tests], width, label='All Local', color='red')

plt.axhline(y=1, color='gray', linestyle='--')

# Labeling the chart
ax.set_ylabel('Relative Performance', fontsize="16")
ax.set_xticks(x)
ax.tick_params(axis='y', labelsize=14)
ax.set_xticklabels(tests, fontsize="16")
ax.legend(fontsize="14")

# Show the chart
if outfile is None:
    plt.show()
else:
    plt.tight_layout()
    plt.savefig(outfile)
