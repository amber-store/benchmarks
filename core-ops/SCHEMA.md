# The shared raw-sample schema

Both drivers write one JSON document with the same shape, so the report pairs
Go and Rust results without knowing anything about either language. The
schema string is `amber-core-ops/samples/1`; `report/validate.py` refuses any
other value.

A profile is measured in two passes of opposite core order, so each core
writes one document per pass. The report merges them: everything that is a
property of the run rather than of a pass — the configuration, the identity,
the environment, the checks, the encodings, the wire inputs — must be
identical in every pass, and a repetition index may not appear twice.

```jsonc
{
  "schema": "amber-core-ops/samples/1",
  "core": "go" | "rust",
  "profile": "quick" | "standard",

  // The whole profile, field for field. The report requires both documents
  // to agree on *every* field, not on a hand-written list: a difference in
  // any of them means the two cores were not asked the same question.
  "config": {
    "name": "standard", "seed": 1592639710, "reps": 7, "warmup": 2,
    "threads_single": 1, "threads_multi": 8,
    "corpus_bytes": 67108864, "tree_files": 1200, "tree_wide": 4000,
    "tree_depth": 24, "synthetic_wide": 40000, "store_objects": 30000,
    "segment_bytes": 16777216, "ref_records": 5000, "inbox_packs": 48,
    "batch_ops": 2000, "payload_total": 8388608,
    "tree_widths": [16, 256, 4096, 40000],
    "tree_depths": [1, 4, 12, 24],
    "fan_outs": [8, 128, 1024, 65536]
  },

  // What the driver was linked against and what produced the samples.
  // `harness_*` is the benchmark repository, kept apart from the core. The
  // report requires these to match the enclosing identity.json.
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
    "num_cpu": 32,              // the machine
    "auto_parallelism": 8,      // what this process actually gets
    "gomaxprocs": 8,            // Rust: "available_parallelism"
    "forced_gc_before_rep": true,   // Go only; false in the Rust driver
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

      // The independent workload dimensions, as numbers. The workload string
      // names the point; this is what a scaling plot reads, and the report
      // requires both cores to declare the same dimensions for the same case.
      "dims": {
        "item_bytes": 4096,   // size of one payload item
        "items": 2048,        // how many of them one measured call processes
        "content": "random",  // random | text | duplicate | empty |
                              // structured | partial-change | tree
        "entries": 4096,      // tree entries the case covered
        "depth": 12,          // directory levels the measured path descended
        "width": 4096,        // widest directory
        "shape": "wide",      // wide | deep | leaf | index | file-index
        "files": 1200,        // on-disk files covered
        "objects": 30000,     // stored objects covered
        "workers": 1,
        "sweep": "item_bytes",  // the dimension this case varies, if any
        "series": "random"      // the family it varies within
      },

      "rep": 0,
      "ops": 2000,                       // core operations in this interval
      "bytes": 8388608,                  // 0 = no rate is meaningful
      "bytes_kind": "payload",           // what those bytes count; see below
      "wall_ns": 1234567,
      "cpu_user_ns": 1100000,            // getrusage delta over the interval
      "cpu_sys_ns": 120000,

      // The whole process's high-water resident set at the end of this
      // repetition. It only ever grows over the lifetime of the driver, so
      // it bounds the case rather than measuring it; it is not a
      // per-operation peak and the report says so.
      "max_rss_kib": 812345,

      // The accumulator the measured calls produced: a fold of their
      // outputs, built in constant time per call. Identical in every
      // repetition of a `stable` case, and identical to the other core's for
      // a `cross_checksum` case.
      "checksum": 10495661784530625,
      "stable": true,
      "cross_checksum": true,
      "status": "ok"
    }
  ],

  // Runtime-specific numbers with no counterpart in the other core. The
  // report prints them in their own table and never pairs them.
  "counters": [
    { "op": "...", "workload": "...", "rep": 0,
      "name": "go.heap_alloc_bytes", "value": 123456 }
  ],

  // How large this core's encoder made a given input. The two cores'
  // compressors legitimately differ, so these are printed side by side and
  // never divided; the report requires the *inputs* to be equal.
  "encodings": [
    { "op": "amberpack.encode_record", "workload": "4KiB-text",
      "logical_bytes": 8388608, "encoded_bytes": 1048576, "items": 2048 }
  ],

  // Every producer's wire pack as this driver read it. Both drivers read the
  // same files, so equal hashes here are the proof that a decode comparison
  // was made on identical bytes.
  "wire_inputs": [
    { "producer": "go", "sha256": "<64 hex>", "bytes": 1288865, "objects": 64 }
  ],

  // Operations this core does not export, or workloads it cannot express.
  // An entry here must have no sample: an absent operation must never be
  // readable as an infinitely fast one, and the report enforces that.
  "unsupported": [
    { "op": "cbor.head_primitives", "workload": "",
      "reason": "Go keeps the CBOR head primitives unexported inside cborx" }
  ],

  // The accumulated result of every measured call, printed so no optimiser
  // can decide the work was dead.
  "blackhole": 105154360425253
}
```

## What `bytes_kind` means

A rate is only as good as its denominator, and these say which one it is.

| Value | Meaning |
|---|---|
| `payload` | Bytes of object payload the operation moved through memory. |
| `logical-scanned` | The logical size of the files a metadata walk covered. The walk reads directory entries and inode metadata, not file bodies, so this measures the tree it traversed, not bandwidth. |
| `included` | Payload bytes of the subset the operation actually included — the filtered tree, the changed files — counted outside the measured interval. |
| `encoded` | Encoded, possibly compressed wire bytes. The two cores' encoders legitimately produce different sizes, so an encoded rate is reported per core and never divided by the other's. |

## Rules the report enforces

Every rule below exists because a run that broke it would still have
produced a plausible-looking table. `report/validate.py` implements them,
`report/test_report.py` tests each refusal, and `report/probe.py` runs the
same probes against a real run's documents.

1. Both documents carry this schema and declare the core they claim to be.
2. Both `config` blocks agree on **every** field either of them has.
3. Both cores have a 40-character revision and a 64-character executable
   hash, **and each raw document records the same ones as `identity.json`**.
4. Both runs saw the same host, architecture, filesystem, extended-attribute
   support and automatic parallelism.
5. Neither check list is empty, no check id appears twice, and no check
   failed.
6. Every check marked `comparable` exists on both sides with an equal,
   non-empty digest, and no comparable check exists on one side only.
7. Neither sample list is empty. Every sample has a finite, positive elapsed
   time and operation count, a non-negative byte count, and a `bytes_kind`
   wherever it reports bytes.
8. Every case has exactly `config.reps` repetitions, each index once, and
   every repetition of a case declares the same threads, operation count,
   byte count and dimensions as its siblings.
9. Every shared case agrees across the two cores on all of those. A shared
   label over different work is refused.
10. A case measured by one core only must belong to an operation the other
    core explicitly lists in `unsupported`, with a reason. A shared case
    missing on one side is a broken pair, not an absent operation.
11. No core both declares an operation unsupported and measures it.
12. Every `stable` case produced the same `checksum` in every repetition, and
    every `cross_checksum` case produced the same one in both cores. A
    single-core case may not claim `cross_checksum`.
13. Both runs read the same wire inputs, by content hash, and every
    per-producer decode case names a producer that was recorded.
14. Both runs' `encodings` cover the same cases and the same inputs; only the
    encoded sizes may differ.
15. The measured set is exactly the coverage matrix's.

A run that fails any of these produces no comparison at all — and the check
happens before anything is written, so there is no window in which a
valid-looking report exists for an invalid run.

## Deriving numbers from a sample

* Batch-normalised latency: `wall_ns / ops`. Short operations are batched, so
  this is the only latency figure that means anything.
* CPU time per core operation: `(cpu_user_ns + cpu_sys_ns) / ops`.
* Operations per second: `ops * 1e9 / wall_ns`.
* Entries or objects per second: `dims.entries * 1e9 / wall_ns`, and likewise
  for `dims.objects`.
* Byte rate: `bytes * 1e9 / wall_ns`, labelled with `bytes_kind`.
* Relative variation: the median absolute deviation over the repetitions,
  divided by the median.

The report never reads a number another program summarised; it recomputes
each of these from the raw samples.
