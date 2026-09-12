# The shared raw-sample schema

Both drivers write one JSON document with the same shape, so the report pairs
Go and Rust results without knowing anything about either language. The
schema string is `amber-core-ops/samples/1`; `report/report.py` refuses any
other value.

```jsonc
{
  "schema": "amber-core-ops/samples/1",
  "core": "go" | "rust",
  "profile": "quick" | "standard",

  // The whole profile, field for field. The report requires both documents
  // to agree on every field: a difference means the two cores were not asked
  // the same question.
  "config": {
    "name": "standard", "seed": 1592639710, "reps": 7, "warmup": 2,
    "threads_single": 1, "threads_multi": 8,
    "corpus_bytes": 67108864, "tree_files": 1200, "tree_wide": 4000,
    "tree_depth": 24, "synthetic_wide": 40000, "store_objects": 30000,
    "segment_bytes": 16777216, "ref_records": 5000, "inbox_packs": 48,
    "batch_ops": 2000
  },

  // What the driver was linked against and what produced the samples.
  // `harness_*` is the benchmark repository, kept apart from the core.
  "identity": {
    "core": "go", "core_repo": "pinned",
    "core_revision": "<40 hex>", "core_dirty": false,
    "core_module": "github.com/amber-store/core", "core_version": "v0.0.7",
    "driver_path": "...", "driver_sha256": "<64 hex>",
    "toolchain": "go1.26.7",
    "harness_revision": "<40 hex>", "harness_dirty": false,
    "build_settings": { "GOARCH": "amd64", "...": "..." }
  },

  "environment": {
    "host": "...", "os": "linux", "arch": "amd64",
    "num_cpu": 8, "gomaxprocs": 8,          // Rust: "available_parallelism"
    "scratch": "...", "scratch_fs": "ext4", "xattrs": true
  },

  "started_at": "2026-09-12T21:00:00.000000000Z",
  "finished_at": "2026-09-12T21:04:00.000000000Z",

  // Correctness, recorded before any timing. A single `passed: false` stops
  // the driver before it measures, and the report refuses the run.
  "checks": [
    {
      "id": "ingest/objects-root",       // the same id in both cores
      "group": "ingest",
      "op": "ingest.objects",
      "passed": true,
      "detail": "",                      // only on failure
      "digest": "0000…",                 // canonical fingerprint of the output
      "comparable": true                 // must equal the other core's digest
    }
  ],

  // One entry per measured repetition of one case.
  "samples": [
    {
      "group": "packstore",
      "op": "packstore.get",             // pairs the two cores
      "workload": "hit",                 // pairs the two cores
      "threads": 1,                      // 0 = the operation chose its own
      "rep": 0,
      "ops": 2000,                       // core operations in this interval
      "bytes": 0,                        // payload bytes, 0 = no throughput
      "wall_ns": 1234567,
      "cpu_user_ns": 1100000,            // getrusage delta over the interval
      "cpu_sys_ns": 120000,
      "max_rss_kib": 812345,             // process high-water after the case
      "status": "ok"
    }
  ],

  // Runtime-specific numbers with no counterpart in the other core. The
  // report prints them in their own table and never pairs them.
  "counters": [
    { "op": "...", "workload": "...", "rep": 0,
      "name": "go.heap_alloc_bytes", "value": 123456 }
  ],

  // Operations this core does not export, or workloads it cannot express.
  // An entry here must have no sample: an absent operation must never be
  // readable as an infinitely fast one, and the report enforces that.
  "unsupported": [
    { "op": "cbor.append_head", "workload": "",
      "reason": "Go keeps the CBOR head primitives unexported inside cborx" }
  ],

  // The accumulated result of every measured call, printed so no optimiser
  // can decide the work was dead.
  "blackhole": 105154360425253
}
```

## Rules the report enforces

1. Both documents carry this schema and declare the core they claim to be.
2. Both `config` blocks are identical, field for field.
3. Both cores have a 40-character revision and a driver hash.
4. No check failed, in either document.
5. Every check marked `comparable` exists on both sides with an equal digest,
   and no comparable check exists on one side only.
6. No operation appears in both `unsupported` and `samples` of the same core.
7. Every sample has a positive elapsed time and a positive operation count.
8. Every case has exactly `config.reps` repetitions on both sides.

A run that fails any of these produces no comparison at all.

## Deriving numbers from a sample

* Time per core operation: `wall_ns / ops`.
* CPU time per core operation: `(cpu_user_ns + cpu_sys_ns) / ops`.
* Throughput: `bytes * 1e9 / wall_ns`, only where `bytes > 0`.

The report never reads a number another program summarised; it recomputes
each of these from the raw samples.
