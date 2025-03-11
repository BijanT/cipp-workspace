#!/bin/bash
timestamp="output_multi_application_$(date +"%m%d%Y_%H%M")"
 
current_dir=$(pwd)
 
memlat_exe=/home/labpc/work/cipp/cipp-workspace/tools/memlat
remote_mem_start_pfn=201326592
memlat_sample_rate=10
 
cipp_exe=/home/labpc/work/cipp/cipp-workspace/tools/cipp
cipp_sample_int=100
cipp_adj_int=9000
cipp_bw_cutoff=250000

numactl_exe=/home/labpc/work/cipp/cipp-workspace/numactl/numactl
damo_exe=/home/labpc/work/cipp/cipp_damo/damo
damo_yaml=/home/labpc/work/cipp/cipp.yaml

demotion_trigger="/sys/kernel/mm/numa/demotion_enabled"
numa_balancing="/proc/sys/kernel/numa_balancing"
 
## for LOOP
strategies=("local" "colloid" "static_lbm" "cipp")
 
## lbm/bwaves Settings
spec_stub="/opt/cpu2017/bin/runcpu --action=run --noreportable --iterations 5 --nobuild  --size ref --tune base --config /opt/cpu2017/gcc-linux-x86.cfg"
bwaves_exe="$spec_stub --threads=32 bwaves_s"
lbm_exe="$spec_stub --threads=64 lbm_s"

# how long to wait between starting bwaves and lbm
sleep_time=150

## output files
lbm_dir="${current_dir}/${timestamp}/lbm"
bwaves_dir="${current_dir}/${timestamp}/bwaves"
latency_dir="${current_dir}/${timestamp}/latency"
cipp_dir="${current_dir}/${timestamp}/cipp"
vmstat_dir="${current_dir}/${timestamp}/vmstat"
 
lbm_file="lbm_output"
bwaves_file="bwaves_output"
latency_file="latency_output"
cipp_file="cipp_output"
vmstat_file="vmstat_output"
 
mkdir -p $lbm_dir $bwaves_dir $latency_dir $vmstat_dir $cipp_dir
 
output_header=(" Strategy" "Trial #" "lbm Time" "bwaves_time")
tabular_header_print="%-15s %-15s %-15s %-15s\n"
tabular_data_print=" %-15s %-15d %15.2f %15.2f\n"
 
current_setting="local"
 
# echo "setting scalling governance"
tuned-adm profile throughput-performance
pushd /opt/cpu2017/
source shrc
popd

 
printf "$tabular_header_print" "${output_header[0]}" "${output_header[1]}" "${output_header[2]}" "${output_header[3]}"
printf "|---------------|---------------|---------------|---------------|---------------|\n"
 
for current_setting in "${strategies[@]}"; do
 
        if [ "$current_setting" = "local" ]; then
                echo '0' > "$demotion_trigger"

                echo '0' > "$numa_balancing"
        elif [ "$current_setting" = "colloid" ]; then
                echo 1 > $demotion_trigger

                echo 6 > $numa_balancing
        else
                echo 0 > $demotion_trigger

                echo 0 > $numa_balancing
        fi
 
        for trial in $(seq 0 2); do
                #sync; echo 3 > /proc/sys/vm/drop_caches;

                vmstat_begin_out_file=${vmstat_dir}/${vmstat_file}_begin_trial_${trial}_${current_setting}.log
                vmstat_end_out_file=${vmstat_dir}/${vmstat_file}_end_trial_${trial}_${current_setting}.log
                lbm_out_file=${lbm_dir}/${lbm_file}_trial_${trial}_${current_setting}.log
                bwaves_out_file=${bwaves_dir}/${bwaves_file}_trial_${trial}_${current_setting}.log
                cipp_out_file=${cipp_dir}/${cipp_file}_trial_${trial}_${current_setting}.log
                latency_out_file=${latency_dir}/${latency_file}_trial_${trial}_${current_setting}.log

                cat /proc/vmstat > ${vmstat_begin_out_file}

                if [ "$current_setting" = "local" ]; then
                        numactl -m 0 $bwaves_exe > ${bwaves_out_file} &
                        bwaves_pid=$!

                        sleep $sleep_time

                        numactl -m 0 $lbm_exe > ${lbm_out_file} &
                        lbm_pid=$!
                elif [ "$current_setting" = "colloid" ]; then
                        taskset -c 127 $memlat_exe $remote_mem_start_pfn $memlat_sample_rate "${latency_out_file}" &
                        memlat_pid=$!

                        $bwaves_exe > ${bwaves_out_file}&
                        bwaves_pid=$!

                        sleep $sleep_time

                        $lbm_exe > ${lbm_out_file}&
                        lbm_pid=$!
                elif [ "$current_setting" = "static_lbm" ]; then
                        echo 65 > /sys/kernel/mm/mempolicy/weighted_interleave/node0
                        echo 35 > /sys/kernel/mm/mempolicy/weighted_interleave/node1

                        $numactl_exe -w 0,1 $bwaves_exe > ${bwaves_out_file} &
                        bwaves_pid=$!

                        sleep $sleep_time

                        $numactl_exe -w 0,1 $lbm_exe > ${lbm_out_file} &
                        lbm_pid=$!
                elif [ "$current_setting" = "cipp" ]; then
                        echo 100 > /sys/kernel/mm/mempolicy/weighted_interleave/node0
                        echo 0 > /sys/kernel/mm/mempolicy/weighted_interleave/node1
                        $damo_exe start $damo_yaml
                        taskset -cp 127 $(pgrep kdamond)

                        $numactl_exe -w 0,1 $bwaves_exe > ${bwaves_out_file} &
                        bwaves_pid=$!

                        $cipp_exe $cipp_sample_int $cipp_adj_int $cipp_bw_cutoff > ${cipp_out_file} &
                        cipp_pid=$!

                        sleep $sleep_time

                        $numactl_exe -w 0,1 $lbm_exe > ${lbm_out_file} &
                        lbm_pid=$!
                fi
 
                while kill -0 $lbm_pid 2>/dev/null; do
                        sleep 0.5
                done
                while kill -0 $bwaves_pid 2>/dev/null; do
                        sleep 0.5
                done

                if [ "$current_setting" = "colloid" ]; then
                        kill -9 $memlat_pid
                elif [ "$current_setting" = "cipp" ]; then
                        kill -9 $cipp_pid
                        $damo_exe stop
                fi

                cat /proc/vmstat > ${vmstat_end_out_file}
 
                lbm_result=$(cat "${lbm_out_file}" | tail -n1 | grep -oP "\d+" | tail -n1)
                bwaves_result=$(cat "${bwaves_out_file}" | tail -n1 | grep -oP "\d+" | tail -n1)

                avg_latency=$(cat "${latency_out_file}" | awk '{sum+=$2; count++} END {print sum/count}')
 
                printf "$tabular_data_print" "$current_setting" "$trial" $lbm_result $bwaves_result $avg_latency
        done

done

