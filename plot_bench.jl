using CSV, DataFrames, Plots

df = CSV.read("results.csv", DataFrame)

# Conversion constant: 1e6 μs = 1 second
const C = 1e6

# Convert Time (ns) to SPS (Steps Per Second)
df.SPS_Mean = C ./ df.Mean
df.SPS_Median = C ./ df.Median

# Propagate error for StdDev: sigma_SPS = sigma_T * (C / Mean_T^2)
df.SPS_StdDev = df.StdDev .* (C ./ (df.Mean .^ 2))

is_fast = maximum(df.SPS_Mean) > 1000

p = plot(df.Threads, df.SPS_Mean,
    ribbon=df.SPS_StdDev,
    label="",
    xlabel="Number of Threads",
    ylabel="SPS",
    fillalpha=0.2,
    linealpha=0,
    color=1,
    xticks=[1:9; 10:2:98],
    yticks=if is_fast
        0:500:1_000_000
    else
        0:100:1000
    end
)

plot!(p, [NaN], [NaN],
    seriestype=:shape,
    label="Mean SPS ± StdDev",
    color=1,
    fillalpha=0.2,
    linealpha=0
)

scatter!(p, df.Threads, df.SPS_Median,
    label="Median SPS",
    marker=:o,
    color=:red
)

savefig(p, "benchmark.svg")
