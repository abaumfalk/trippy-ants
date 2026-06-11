#!/bin/bash

# Usage: ./bench_rayon.sh [duration] [max_threads]
DURATION=${1:-30}
MAX_THREADS=${2:-64}

echo "Threads,Mean,Median,StdDev" > results.csv

echo "Building binary..."
cargo b --release

echo "Starting benchmark (${DURATION}s per test, max threads: ${MAX_THREADS})..."

for threads in $(seq 1 "$MAX_THREADS"); do
    printf "Threads %-2d... " "$threads"

    # 1. Calculate total threads (1 master + N worker threads)
    total_threads=$((threads + 1))

    # 2. Determine optimal core pinning based on 5950X topology
    if [ "$total_threads" -le 16 ]; then
        # Map to physical cores only (0 to total_threads-1)
        core_list="0-$((total_threads - 1))"
    elif [ "$total_threads" -le 32 ]; then
        # Physical cores exhausted; spill over into SMT siblings (16 to total_threads-1)
        core_list="0-15,16-$((total_threads - 1))"
    else
        # Cap at max logical cores to prevent taskset affinity errors
        core_list="0-31"
    fi

    # 3. Execute with taskset
    raw_output=$(RAYON_NUM_THREADS=$threads taskset -c "$core_list" timeout "${DURATION}s" ./target/release/trippy-ants 2>&1 | grep "Mean:")
    
    stats_line=$(echo "$raw_output" | tail -n 1)
    
    mean=$(echo "$stats_line" | awk -F '|' '{print $1}' | sed 's/Mean://g' | xargs)
    median=$(echo "$stats_line" | awk -F '|' '{print $2}' | sed 's/Median://g' | xargs)
    stddev=$(echo "$stats_line" | awk -F '|' '{print $3}' | sed 's/StdDev://g' | xargs)
    median_fps=$(awk "BEGIN {print 1e6 / $median}")

    if [ -z "$median" ]; then
        echo "FAILED"
    else
        # Appended the core_list to the terminal output so you can verify the pinning
        echo "Mean: $mean, Median: $median, StdDev: $stddev, Median FPS: $median_fps (Cores: $core_list)"
        echo "$threads,$mean,$median,$stddev" >> results.csv
    fi
done

echo "Benchmark complete. Results saved to results.csv"

echo "Producing Plot..."
julia plot_bench.jl

echo "Plot saved to benchmark.svg"