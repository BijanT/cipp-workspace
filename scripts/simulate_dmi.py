#!/usr/bin/env python3

import sys
import os
import re
import matplotlib
import matplotlib.pyplot as plt
import csv

from helpers import eprint

def dmi_adjust_ratio(last_bw, cur_bw, cur_ratio, last_step):
    MIN_STEP = 2
    MAX_STEP = 8
    last_step_good = cur_bw > last_bw
    bw_change = float(last_bw - cur_bw) / last_bw

    if cur_bw < 250:
        step = abs(last_step)
    elif last_step == 0:
        step = int(cur_ratio * bw_change)
        if abs(step) < 2 * MIN_STEP:
            step = 0
    elif last_step_good:
        damping = abs(bw_change * 100 / last_step)
        print(str(last_step) + " " + str(damping))
        if damping < 1:
            step = int(damping * last_step)
        else:
            step = last_step
    else:
        step = int(-last_step / 2)

    if abs(step) < MIN_STEP:
        step = 0
    elif abs(step) > MAX_STEP:
        step = MAX_STEP if step > 0 else -MAX_STEP

    ratio = cur_ratio + step
    if ratio > 100:
        ratio = 100
    elif ratio < 0:
        ratio = 0

    return (ratio, step)

def read_bws_file(filename):
    bws = {}
    with open(filename, "r") as csvfile:
        reader = csv.DictReader(csvfile)
        for row in reader:
            ratio = int(row["DRAM Ratio"])
            bw = float(row["BW"])
            bws[ratio] = bw

    return bws

def get_bw(bws, ratio):
    lower = 0
    greater = 0
    for r in bws:
        if r < ratio:
            lower = r
        elif r == ratio:
            return bws[r]
        else:
            greater = r
            break

    percent_greater = float((ratio - lower) / (greater - lower))
    percent_lower = 1.0 - percent_greater
    return (bws[greater] * percent_greater) + (bws[lower] * percent_lower)

# Have defaults for the bw data
first_bws = {
    0: 209.822,
    10: 236.041,
    20: 270.705,
    30: 333.98,
    40: 421.738,
    50: 538.822,
    60: 647.731,
    65: 654.157,
    67: 666.453,
    70: 669.857,
    75: 669.678,
    77: 665,
    80: 640.375,
    90: 575.409,
    100: 529.604
}
second_bws = {
    0: 209.822,
    55: 292.283,
    65: 325.85,
    75: 354.692,
    77: 378,
    80: 386.703,
    83: 393.209,
    85: 402.048,
    88: 404.946,
    90: 400.348,
    92: 397.986,
    95: 391.929,
    100: 371.088,
}

outfile = None
if len(sys.argv) >= 2:
    outfile = sys.argv[1]

if len(sys.argv) >= 4:
    first_bws_file = sys.argv[2]
    second_bws_file = sys.argv[3]
    first_bws = read_bws_file(first_bws_file)
    second_bws = read_bws_file(second_bws_file)

bws = first_bws

simul_ratios = [100]
simul_steps = [0]
simul_bws = [1]

while True:
    if bws == second_bws and simul_ratios[-1] == simul_ratios[-2]:
        break
    if bws == first_bws and len(simul_ratios) > 2 and simul_ratios[-1] == simul_ratios[-2]:
        # Fake the workload spending some time at this ratio
        # before changing the workload characteristics
        for _ in range(3):
            simul_ratios.append(simul_ratios[-1])
            simul_steps.append(simul_steps[-1])
            simul_bws.append(simul_bws[-1])
        bws = second_bws

    cur_bw = get_bw(bws, simul_ratios[-1])
    (new_ratio, new_step) = dmi_adjust_ratio(simul_bws[-1], cur_bw, simul_ratios[-1], simul_steps[-1])

    simul_ratios.append(new_ratio)
    simul_steps.append(new_step)
    simul_bws.append(cur_bw)

print("Ratios:")
print(simul_ratios)
print("Steps:")
print(simul_steps)
print("BWs:")
print(simul_bws)

fig, ratio_ax = plt.subplots(figsize=(10,6))
bw_ax = ratio_ax.twinx()

ratio_xs = range(len(simul_ratios))
bw_xs = [x - 0.5 for x in ratio_xs]

ln1 = ratio_ax.plot(ratio_xs, simul_ratios, linewidth=2.0, label="Interleave Ratio")
ln2 = bw_ax.plot(bw_xs, simul_bws, linewidth=2.0, color="red", label="Bandwidth")

# Combine line and label information for the legend
lines = ln1 + ln2
labels = [l.get_label() for l in lines]
ratio_ax.legend(lines, labels, fontsize=18, loc="lower right")

ratio_ax.set_xlabel("Calibration Period", fontsize=22)
ratio_ax.set_ylabel("Interleave Ratio", fontsize=22)
ratio_ax.tick_params(labelsize=18)
ratio_ax.set_ylim([0,105])
bw_ax.set_ylabel("Bandwidth (GB/s)", fontsize=22)
bw_ax.set_ylim([0, 720])
bw_ax.tick_params(labelsize=18)

if outfile is not None:
    plt.savefig(outfile)
else:
    plt.show()

