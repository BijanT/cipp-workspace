#!/usr/bin/env python3

import sys
import os
import re
import csv

from helpers import (eprint,get_clover_runtime,get_avg_gapbs_time,
    get_stream_triad,get_spec_time)

results_path = os.path.abspath(sys.argv[1])

results_files = [f for f in os.listdir(results_path) if os.path.isfile(os.path.join(results_path, f))]

exp_info_pattern = re.compile(r"wkld_output_ratio_(\d+)_cpu_(\d+)_(\w+)")
csv_fieldnames = ['Strategy', 'Workload', 'Cores', 'Ratio', 'Result', 'File']
csv = csv.DictWriter(sys.stdout, fieldnames=csv_fieldnames)
csv.writeheader()

for file in results_files:
    m = exp_info_pattern.match(file)

    ratio = m.group(1)
    cores = m.group(2)
    workload = m.group(3)
    strategy = "DMI"
    filename = results_path + "/" + file

    if workload == "clover" or workload == "cloverleaf":
        result = get_clover_runtime(filename)
    elif workload == "pr" or workload == "bc" or workload == "bfs":
        result = get_avg_gapbs_time(filename)
    elif workload == "stream":
        result = get_stream_triad(filename)
    elif workload == "bwaves_s" or workload == "lbm_s":
        result = get_spec_time(filename)
    else:
        print("Invalid workload " + workload)
        exit()

    csv.writerow({
        csv_fieldnames[0]: strategy,
        csv_fieldnames[1]: workload,
        csv_fieldnames[2]: cores,
        csv_fieldnames[3]: ratio,
        csv_fieldnames[4]: result,
        csv_fieldnames[5]: filename,
    })


