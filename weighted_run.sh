#!/bin/bash
timestamp="output_numactl_$(date +"%m%d%Y_%H%M")"
 
current_dir=$(pwd)
 
bwmon_exe=/home/labpc/work/cipp/cipp-workspace/tools/bwmon
bwmon_sample_rate=100

demotion_trigger="/sys/kernel/mm/numa/demotion_enabled"
numa_balancing="/proc/sys/kernel/numa_balancing"
 
 
## for LOOP
#workloads=("cloverleaf" "pr" "bfs" "bc" "stream" "bwaves_s" "lbm_s")
workloads=("cloverleaf" "bwaves_s" "lbm_s")
cpu_core_list=($(seq 30 30 120))
local_ratio_list=($(seq 60 5 100))
cpu_core_list[-1]=119
rsvd_core=119
 
## CloverLeaf Settings
clover_exe=/home/labpc/work/cipp/CloverLeaf/build/omp-cloverleaf
clover_input_file=/home/labpc/work/cipp/CloverLeaf/InputDecks/clover_bm256_300.in

## GAPBS Settings
pr_exe=/home/labpc/work/cipp/gapbs/pr
bc_exe=/home/labpc/work/cipp/gapbs/bc
bfs_exe=/home/labpc/work/cipp/gapbs/bfs
 
## Stream Settings
stream_exe=/home/labpc/work/cipp/stream/stream

## SPEC
spec_stub="/opt/cpu2017/bin/runcpu --action=run --noreportable --iterations 5 --nobuild  --size ref --tune base --config /opt/cpu2017/gcc-linux-x86.cfg"

numactl_exe=/home/labpc/work/cipp/cipp-workspace/numactl/numactl

## output files
wkld_dir="${current_dir}/${timestamp}/wkld"
bwmon_dir="${current_dir}/${timestamp}/bwmon"
vmstat_dir="${current_dir}/${timestamp}/vmstat"
 
wkld_file="wkld_output"
bwmon_file="bwmon_output"
vmstat_file="vmstat_output"
pgmigrate_file="pgmigrate_output"
 
mkdir -p $wkld_dir $bwmon_dir $vmstat_dir
 
output_header=(" Workload" "Core Count" "Local Ratio" "Result" "Avg BW")
tabular_header_print="%-15s %-15s %-15s %-15s %-15s\n"
tabular_data_print=" %-15s %-15d %-15d %15.2f %15.2f\n"
 
current_core=128
current_wkld="cloverleaf"
 
# echo "setting scalling governance"
tuned-adm profile throughput-performance
echo 0 > $numa_balancing
echo 0 > $demotion_trigger
 
pushd /opt/cpu2017/
source shrc
popd

printf "$tabular_header_print" "${output_header[0]}" "${output_header[1]}" "${output_header[2]}" "${output_header[3]}" "${output_header[4]}"
printf "|---------------|---------------|---------------|---------------|---------------|\n"
 
for current_wkld in "${workloads[@]}"; do
 
        if [ "$current_wkld" = "cloverleaf" ]; then
                wkld_cmd="$clover_exe --file $clover_input_file"
        elif [ "$current_wkld" = "pr" ]; then
                wkld_cmd="$pr_exe -g 30"
        elif [ "$current_wkld" = "bc" ]; then
                wkld_cmd="$bc_exe -g 30"
        elif [ "$current_wkld" = "bfs" ]; then
                wkld_cmd="$bfs_exe -g 30"
        else
                wkld_cmd=$stream_exe
        fi
 
        for current_core in "${cpu_core_list[@]}"; do
                for current_ratio in "${local_ratio_list[@]}"; do
                        #sync; echo 3 > /proc/sys/vm/drop_caches;

                        vmstat_begin_out_file=${vmstat_dir}/${vmstat_file}_begin_ratio_${current_ratio}_cpu_${current_core}_${current_wkld}.log
                        vmstat_end_out_file=${vmstat_dir}/${vmstat_file}_end_ratio_${current_ratio}_cpu_${current_core}_${current_wkld}.log
                        pgmigrate_out_file=${vmstat_dir}/${pgmigrate_file}_ratio_${current_ratio}_cpu_${current_core}_${current_wkld}.log
                        wkld_out_file=${wkld_dir}/${wkld_file}_ratio_${current_ratio}_cpu_${current_core}_${current_wkld}.log
                        bwmon_out_file=${bwmon_dir}/${bwmon_file}_ratio_${current_ratio}_cpu_${current_core}_${current_wkld}.log

                        cat /proc/vmstat > ${vmstat_begin_out_file}

                        echo ${current_ratio} > /sys/kernel/mm/mempolicy/weighted_interleave/node0
                        echo $((100 - current_ratio)) > /sys/kernel/mm/mempolicy/weighted_interleave/node1

                        if [ "$current_wkld" = "bwaves_s" ] || [ "$current_wkld" = "lbm_s" ]; then
                                $numactl_exe -w 0,1 $spec_stub --threads=${current_core} $current_wkld > ${wkld_out_file} &
                        else
                                $numactl_exe -w 0,1 taskset -c 0-$((current_core - 1)) $wkld_cmd > ${wkld_out_file} &
                        fi

                        wkld_pid=$!
 
                        taskset -c $rsvd_core $bwmon_exe $bwmon_sample_rate "${bwmon_out_file}" $wkld_pid &

                        bwmon_pid=$!

                        # echo "touched latency file core count $current_core, setting $current_wkld"

                        while kill -0 $bwmon_pid 2>/dev/null; do
                                cat /proc/vmstat | grep "\(pgmigrate_success\|pgdemote\)" >> ${pgmigrate_out_file}
                                sleep 1
                        done
                        cat /proc/vmstat > ${vmstat_end_out_file}
 
                        if [ "$current_wkld" = "cloverleaf" ]; then
                                perf_result=$(cat "${wkld_out_file}" | grep "Wall clock" | tail -n1 | grep -oP '\d+\.\d+')
                        elif [ "$current_wkld" = "pr" ] || [ "$current_wkld" = "bc" ] || [ "$current_wkld" = "bfs" ]; then
                                perf_result=$(cat "${wkld_out_file}" | grep "Average Time" | grep -oP '\d+\.\d+')
                        elif [ "$current_wkld" = "stream" ]; then
                                perf_result=$(cat "${wkld_out_file}" | grep "Triad" | grep -oP '\d+\.\d+' | head -n1)
                        else
                                perf_result=$(cat "${wkld_out_file}" | tail -n1 | grep -oP "\d+" | tail -n1)
                        fi

                        avg_bw=$(grep 'Aggregate' "${bwmon_out_file}" | awk '{sum+=$3; count++} END {print sum/count}')
 
                        printf "$tabular_data_print" "$current_wkld" "$current_core" "$current_ratio" $perf_result $avg_bw
                done
        done

done

