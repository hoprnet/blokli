# Heap profiling a Blokli sync

The `bloklid-heap-profiler` Docker image is a diagnostic-only image. It uses
jemalloc with allocation profiling enabled and keeps line-level symbols in the
binary. The normal `bloklid` image and its allocator are unchanged.

Deploy the profiling image with a disk-backed volume for profiles (not a
`medium: Memory` `emptyDir`) and the following environment variable:

```yaml
env:
  - name: MALLOC_CONF
    value: prof:true,prof_active:true,lg_prof_sample:19,prof_gdump:true,prof_final:true,prof_prefix:/profiles/jeprof
volumeMounts:
  - name: heap-profiles
    mountPath: /profiles
volumes:
  - name: heap-profiles
    emptyDir: {}
```

`prof_gdump:true` writes a snapshot whenever the live heap reaches a new high
water mark, which captures the growth throughout a sync. `prof_final:true`
writes a final snapshot during a graceful shutdown. The samples use a 512 KiB
interval (`lg_prof_sample:19`), keeping profiling overhead suitable for this
diagnostic run.

The profiling image handles `SIGUSR1` in a dedicated task, so it writes an
immediate snapshot even while fast sync is running. This is the preferred way
to capture a profile during syncing:

```sh
kubectl -n hopr exec <pod> -- kill -USR1 1
```

The log line includes the exact path of the created file. Do not use `SIGKILL`:
it cannot write a final profile.

After the sync, copy `/profiles/jeprof.*.heap` from the pod and inspect a
snapshot with the matching unstripped `bloklid` binary from the profiling image.
The profiling image includes `tar`, so Kubernetes can copy the directory:

```sh
kubectl -n hopr cp <pod>:/profiles ./blokli-heap-profiles
```

Inspect a snapshot with `jeprof`:

```sh
jeprof --show_bytes --text /path/to/bloklid /path/to/jeprof.*.heap
jeprof --show_bytes --svg /path/to/bloklid /path/to/jeprof.*.heap > heap.svg
```

The report attributes live allocations to Rust call stacks. Compare a snapshot
near the start of syncing with one taken after the memory plateau; allocations
that persist in the latter are the retained heap. Do not use this image for
normal production workloads or expose the generated profiles outside the
investigation, since they contain process allocation metadata.
