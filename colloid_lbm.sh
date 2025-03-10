#!/bin/bash
timestamp="output_colloid_lbm_$(date +"%m%d%Y_%H%M")"
 
current_dir=$(pwd)
 
bwmon_exe=/home/labpc/work/cipp/cipp-workspace/tools/bwmon
bwmon_sample_rate=200

memlat_exe=/home/labpc/work/cipp/cipp-workspace/tools/memlat
remote_mem_start_pfn=201326592
memlat_sample_rate=10
 
demotion_trigger="/sys/kernel/mm/numa/demotion_enabled"
numa_balancing="/proc/sys/kernel/numa_balancing"
 
 
## for LOOP
local_remote=("local" "colloid")
cpu_core_list=($(seq 32 32 128))
 
## lbm Settings
spec_stub="/opt/cpu2017/bin/runcpu --action=run --noreportable --iterations 5 --nobuild  --size ref --tune base --config /opt/cpu2017/gcc-linux-x86.cfg"

## output files
lbm_dir="${current_dir}/${timestamp}/lbm"
bwmon_dir="${current_dir}/${timestamp}/bwmon"
latency_dir="${current_dir}/${timestamp}/latency"
vmstat_dir="${current_dir}/${timestamp}/vmstat"
 
lbm_file="lbm_output"
bwmon_file="bwmon_output"
latency_file="latency_output"
vmstat_file="vmstat_output"
 
mkdir -p $lbm_dir $bwmon_dir $latency_dir $vmstat_dir
 
output_header=(" Strategy" "Core Count" "Trial #" "Time" "Avg BW" "Avg Latency")
tabular_header_print="%-15s %-15s %-15s %-15s %-15s %-15s\n"
tabular_data_print=" %-15s %-15d %-15d %15.2f %15.2f %-15.2f\n"
 
current_core=128
current_setting="local"
 
# echo "setting scalling governance"
tuned-adm profile throughput-performance
pushd /opt/cpu2017/
source shrc
popd

 
printf "$tabular_header_print" "${output_header[0]}" "${output_header[1]}" "${output_header[2]}" "${output_header[3]}" "${output_header[4]}" "${output_header[5]}"
printf "|---------------|---------------|---------------|---------------|---------------|---------------|\n"
 
for current_setting in "${local_remote[@]}"; do
 
        if [ "$current_setting" = "local" ]; then
                echo '0' > "$demotion_trigger"

                echo '0' > "$numa_balancing"
        elif [ "$current_setting" = "colloid" ]; then
                echo 1 > $demotion_trigger

                echo 6 > $numa_balancing
        else
                echo 1 > $demotion_trigger

                echo 2 > $numa_balancing
        fi
 
        for current_core in "${cpu_core_list[@]}"; do
                for trial in $(seq 0 2); do
                        #sync; echo 3 > /proc/sys/vm/drop_caches;

                        vmstat_begin_out_file=${vmstat_dir}/${vmstat_file}_begin_trial_${trial}_cpu_${current_core}_${current_setting}.log
                        vmstat_end_out_file=${vmstat_dir}/${vmstat_file}_end_trial_${trial}_cpu_${current_core}_${current_setting}.log
                        lbm_out_file=${lbm_dir}/${lbm_file}_trial_${trial}_cpu_${current_core}_${current_setting}.log
                        bwmon_out_file=${bwmon_dir}/${bwmon_file}_trial_${trial}_cpu_${current_core}_${current_setting}.log
                        latency_out_file=${latency_dir}/${latency_file}_trial_${trial}_cpu_${current_core}_${current_setting}.log

                        cat /proc/vmstat > ${vmstat_begin_out_file}

                        if [ "$current_setting" = "local" ]; then

                                numactl -m 0 $spec_stub --threads=${current_core} lbm_s > ${lbm_out_file} &

                        else

                                $spec_stub --threads=${current_core} lbm_s > ${lbm_out_file}&

                        fi

                        lbm_pid=$!
 
                        $bwmon_exe $bwmon_sample_rate "${bwmon_out_file}" $lbm_pid &

                        bwmon_pid=$!
 
                        $memlat_exe $remote_mem_start_pfn $memlat_sample_rate "${latency_out_file}" &

                        memlat_pid=$!

                        # echo "touched latency file core count $current_core, setting $current_setting"

                        while kill -0 $bwmon_pid 2>/dev/null; do
                                sleep 0.5
                        done
                        kill -9 $memlat_pid
                        cat /proc/vmstat > ${vmstat_end_out_file}
 
                        lbm_result=$(cat "${lbm_out_file}" | tail -n1 | grep -oP "\d+" | tail -n1)

                        avg_bw=$(grep 'Aggregate' "${bwmon_out_file}" | awk '{sum+=$3; count++} END {print sum/count}')
 
                        avg_latency=$(cat "${latency_out_file}" | awk '{sum+=$2; count++} END {print sum/count}')
 
                        printf "$tabular_data_print" "$current_setting" "$current_core" "$trial" $lbm_result $avg_bw $avg_latency
                done
        done

done

