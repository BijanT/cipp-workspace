#!/bin/bash
timestamp="output_cipp_$(date +"%m%d%Y_%H%M")"
 
current_dir=$(pwd)
 
bwmon_exe=/home/labpc/work/cipp/cipp-workspace/tools/bwmon
bwmon_sample_rate=200

cipp_exe=/home/labpc/work/cipp/cipp-workspace/tools/cipp
cipp_sample_int=200
cipp_adj_int=6000
cipp_bw_cutoff=200000

demotion_trigger="/sys/kernel/mm/numa/demotion_enabled"
numa_balancing="/proc/sys/kernel/numa_balancing"
 
 
## for LOOP
workloads=("cloverleaf" "pr" "stream")
cpu_core_list=($(seq 32 32 128))
 
## CloverLeaf Settings
clover_exe=/home/labpc/work/cipp/CloverLeaf/build/omp-cloverleaf
clover_input_file=/home/labpc/work/cipp/CloverLeaf/InputDecks/clover_bm256_300.in

## PR Settings
pr_exe=/home/labpc/work/cipp/gapbs/pr
 
## Stream Settings
stream_exe=/home/labpc/work/cipp/stream/stream

## output files
wkld_dir="${current_dir}/${timestamp}/wkld"
bwmon_dir="${current_dir}/${timestamp}/bwmon"
cipp_dir="${current_dir}/${timestamp}/cipp"
vmstat_dir="${current_dir}/${timestamp}/vmstat"
 
wlkd_file="wlkd_output"
bwmon_file="bwmon_output"
cipp_file="cipp_output"
vmstat_file="vmstat_output"
 
mkdir -p $wkld_dir $bwmon_dir $latency_dir $vmstat_dir
 
output_header=(" Workload" "Core Count" "Trial #" "Avg Time" "Avg BW")
tabular_header_print="%-15s %-15s %-15s %-15s %-15s\n"
tabular_data_print=" %-15s %-15d %-15d %15.2f %15.2f\n"
 
current_core=128
current_wkld="cloverleaf"
 
# echo "setting scalling governance"
tuned-adm profile throughput-performance
echo 0 > $numa_balancing
echo 0 > $demotion_trigger
 
printf "$tabular_header_print" "${output_header[0]}" "${output_header[1]}" "${output_header[2]}" "${output_header[3]}" "${output_header[4]}"
printf "|---------------|---------------|---------------|---------------|---------------|\n"
 
for current_wkld in "${workloads[@]}"; do
 
        if [ "$current_wkld" = "cloverleaf" ]; then
                wkld_cmd="$clover_exe --file $clover_input_file"
        elif [ "$current_setting" = "pr" ]; then
                wkld_cmd="$pr_exe -g 30"
        else
                wkld_cmd=$stream_exe
        fi
 
        for current_core in "${cpu_core_list[@]}"; do
                for trial in $(seq 0 2); do
                        #sync; echo 3 > /proc/sys/vm/drop_caches;

                        vmstat_begin_out_file=${vmstat_dir}/${vmstat_file}_begin_trial_${trial}_cpu_${current_core}_${current_setting}.log
                        vmstat_end_out_file=${vmstat_dir}/${vmstat_file}_end_trial_${trial}_cpu_${current_core}_${current_setting}.log
                        wkld_out_file=${clover_dir}/${wkld_file}_trial_${trial}_cpu_${current_core}_${current_wkld}.log
                        bwmon_out_file=${bwmon_dir}/${bwmon_file}_trial_${trial}_cpu_${current_core}_${current_setting}.log
                        cipp_out_file=${cipp_dir}/${cipp_file}_trial_${trial}_cpu_${current_core}_${current_setting}.log

                        cat /proc/vmstat > ${vmstat_begin_out_file}

                        echo 100 > /sys/kernel/mm/mempolicy/weighted_interleave/node0
                        echo 0 > /sys/kernel/mm/mempolicy/weighted_interleave/node1

                        $numactl_exe -w 0,1 taskset -c 0-$((current_core - 1)) $wkld_cmd > ${wkld_out_file} &

                        wkld_pid=$!
 
                        $bwmon_exe $bwmon_sample_rate "${bwmon_out_file}" $wkld_pid &

                        bwmon_pid=$!

                        $cipp_exe $cipp_sample_int $cipp_adj_int $cipp_bw_cutoff > ${cipp_out_file} &

                        cipp_pid=$!

                        # echo "touched latency file core count $current_core, setting $current_setting"

                        while kill -0 $bwmon_pid 2>/dev/null; do
                                sleep 0.5
                        done
                        kill $cipp_pid
                        cat /proc/vmstat > ${vmstat_end_out_file}
 
                        if [ "$current_wkld" = "cloverleaf" ]; then
                                perf_result=$(cat "${wkld_out_file}" | grep "Wall clock" | tail -n1 | grep -oP '\d+\.\d+')
                        elif [ "$current_setting" = "pr" ]; then
                                perf_result=$(cat "${wkld_out_file}" | grep "Average Time" | grep -oP '\d+\.\d+')
                        else
                                perf_result=$(cat "${wkld_out_file}" | grep "Triad" | tail -n1 | grep -oP '\d+\.\d+')
                        fi

                        avg_bw=$(grep 'Aggregate' "${bwmon_out_file}" | awk '{sum+=$3; count++} END {print sum/count}')
 
                        printf "$tabular_data_print" "$current_wkld" "$current_core" "$trial" $perf_result $avg_bw
                done
        done

done

