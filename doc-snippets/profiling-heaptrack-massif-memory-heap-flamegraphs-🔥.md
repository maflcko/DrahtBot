# perf/flamegraphs

Make sure to install `perf`, (use GCC -O1 ?) then:

```
# Attach to running process
perf record -g --call-graph dwarf/fp/lbr --freq=max -p PID -- sleep 60

# For full lifetime of process
FUZZ=rpc  perf record -g --call-graph fp/lbr/dwarf --freq=max  ./bld-cmake/bin/fuzz -runs=1 ~/Downloads/clusterfuzz-testcase-rpc-5441640485683200
```

```
hotspot  ./perf.data
```

Make sure to clone https://github.com/brendangregg/FlameGraph, then:

```
( perf script | ./FlameGraph/stackcollapse-perf.pl --all | ./FlameGraph/flamegraph.pl > /tmp/a.svg ) && firefox /tmp/a.svg
```


# massif

```
$ FUZZ=system valgrind --tool=massif ./src/test/fuzz/fuzz ../btc_qa_assets/fuzz_seed_corpus/system/
^C
$ massif-visualizer ./massif.out.952024
```

See also:

* https://github.com/KDE/heaptrack#comparison-to-valgrinds-massif
* https://github.com/gperftools/gperftools
