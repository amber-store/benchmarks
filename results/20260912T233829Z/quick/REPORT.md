# Amber core operations: Go vs Rust (quick profile)

Both cores measured through their **library APIs, in process**. No command
line is started inside a measured interval.

There is no overall ranking in this report, and none is intended: the two
implementations trade differently across the operation set, and a single
number over unlike operations would say nothing.

## Provenance

| | Go core | Rust core |
|---|---|---|
| Module | `github.com/amber-store/core` | `amber-store-core` |
| Commit | `4ed4660657b12421a534ab0b08cfd717ae3d2291` | `141df2b0a9a8ad1726769f63811db3a166be188a` |
| Source | pinned | pinned |
| Working tree dirty | false | false |
| Driver SHA-256 | `dcfde5bf73dc09ce186379f0395214385bfb092f3eee60abe584155bd5ed0efb` | `a6076fc943b3f007b008a178e7ad0769f657c05ce11635975638158fe9bcae47` |
| Toolchain | go1.26.7 | rustc 1.95.0 (59807616e 2026-04-14) (built from a source tarball) |

* Pinned by `flake.lock`: Go `4ed4660657b12421a534ab0b08cfd717ae3d2291`, Rust `141df2b0a9a8ad1726769f63811db3a166be188a`.
* Pin verified before measuring: **true**. Every build-relevant file of the resolved Go module was compared against the pinned checkout (`identical`), and the Rust core was resolved from `git+https://github.com/amber-store/core-rs?rev=141df2b0a9a8ad1726769f63811db3a166be188a#141df2b0a9a8ad1726769f63811db3a166be188a`, whose requested and resolved commits are `141df2b0a9a8ad1726769f63811db3a166be188a` and `141df2b0a9a8ad1726769f63811db3a166be188a`.
* Benchmark harness revision `40ac759a5b23035e213e5434deaa8be5041d3bea` (dirty: false), sources SHA-256 `9b8c3e8623b8dc96b0657237f4c0940d1d78ea6d35e9ec16602256fbea8756f3` — recorded before the run, re-checked after it and before this report was written. The harness revision is deliberately kept apart from the core revisions above.

## Environment and configuration

* Host `bld1`, Linux 6.18.45, AMD Ryzen 9 7950X3D 16-Core Processor, 32 CPUs, 128324 MiB RAM.
* Both drivers pinned to CPUs `0-7` and run **sequentially**, never concurrently.
* Operations that choose their own parallelism (marked `auto`) got 8 workers in both cores, under that CPU set.
* Scratch on `ext4`; extended attributes in fixtures: true.
* Profile `quick`: 1 measured repetitions after 0 warm-up passes, seed `1592639710`.
* Fixtures: 4 MiB corpora, 120 tree files, 200 wide entries, depth 8, 1500 store objects, 2 MiB segments, 200 references, 8 inbox packs.
* Swept dimensions: payload sizes 0 B, 64 B, 4.00 KiB, 1.00 MiB, 8.00 MiB crossed with random, compressible and duplicate content; directory widths [16, 256, 4096, 40000]; path depths [1, 4, 12, 24]; file-index fan-outs [8, 128, 1024, 65536]. These are the same absolute points in every profile.
* Caches were **not** dropped: every measurement is warm. No cache-cold claim is made anywhere in this report.
* The Go driver runs a full garbage collection before every measured repetition (`forced_gc_before_rep`: true); the Rust driver has no collector to run (false). That is an asymmetry, not a symmetry: it removes the previous case's garbage from the Go measurement and has no counterpart on the other side.

## Validity

None of the tables below exists unless every statement here holds. The rules are
in `report/validate.py`, and each of them refuses a comparison that would have
looked plausible.

* both runs carry the shared raw-sample schema
* both runs used the same profile and all 20 configuration fields
* both cores are identified by a full commit id and an executable hash, and each raw document carries the same identity
* both runs saw the same host, filesystem, extended-attribute support and automatic parallelism (8)
* every correctness check passed (183 Go, 184 Rust), and no check id is recorded twice
* all 171 cross-core output digests are identical
* every sample records a finite, positive elapsed time and operation count, and every byte count says what it counts
* every case has exactly repetitions 0..0 on both sides, each once
* every repetition of a case describes the same work as its siblings
* all 247 shared cases agree on threads, operation count, byte count and every workload dimension
* a case measured by one core only belongs to an operation the other core explicitly declares unsupported
* no core both declares an operation unsupported and measures it, and every declaration carries a reason
* every repetition of each of the 243 stable cases produced the same output
* the two cores produced byte-identical output in all 160 cases that require it
* both cores decoded the same bytes: 2 wire inputs, identical content hashes
* the two cores encoded the same input in all 14 recorded encodings; their output sizes are reported separately and never divided
* the measured set is exactly the manifest's: 100 paired operations over 247 workloads, and every check it names was recorded

## Coverage

* Paired operation/workload cases: **247**
* Distinct paired operations: **100**
* Single-core cases (reported separately, never compared): **6**
* Correctness checks: **183** Go, **184** Rust, of which **171** assert that the two cores produced the same output.
* Scaling sweeps drawn: **27**; worker-scaling pairs: **8**.
* The measured set is exactly the coverage matrix's: 100 paired operations over 247 workloads. See `../../core-ops/COVERAGE.md`.

## Paired results

`ns/op` is the measured interval divided by the number of core operations in it —
short operations are batched, so this is the only latency figure that means
anything. `var` is the median absolute deviation as a fraction of the median: the
one dispersion figure comparable between a nanosecond case and a second-long one.
The ratio is Go median ÷ Rust median with a 95 % percentile-bootstrap interval,
and *p* is a two-sided Mann-Whitney U test over the per-repetition values.

> **This profile measures 1 repetition(s) per case.** Fewer than
> three has no dispersion and no test statistic, so the ratio column below is
> one observation divided by another and the CI, variation and *p* columns are
> omitted. The `quick` profile is for checking correctness fast; use
> `--profile standard` for anything that will be quoted.

### `amberignore`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `amberignore.descend` | 8-subdirs | 1 | 1 | 5.311 us | — | 3.281 us | — | 1.62× | — | — |
| `amberignore.ignored` | mixed-names | 1 | 1 | 205.8 ns | — | 111.4 ns | — | 1.85× | — | — |
| `amberignore.ignored` | nil-matcher | 1 | 1 | 5.4 ns | — | 0.2 ns | — | 25.20× | — | — |
| `amberignore.root` | load-root | 1 | 1 | 7.585 us | — | 4.453 us | — | 1.70× | — | — |

### `amberpack`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `amberpack.decode_payload` | mixed-records/producer-go | 1 | 1 | 16.108 us | — | 10.313 us | — | 1.56× | — | — |
| `amberpack.decode_payload` | mixed-records/producer-rust | 1 | 1 | 14.528 us | — | 9.411 us | — | 1.54× | — | — |
| `amberpack.encode_record` | 1MiB-duplicate | 1 | 1 | 603.874 us | — | 285.536 us | — | 2.11× | — | — |
| `amberpack.encode_record` | 1MiB-random | 1 | 1 | 517.950 us | — | 278.252 us | — | 1.86× | — | — |
| `amberpack.encode_record` | 1MiB-text | 1 | 1 | 3.911 ms | — | 1.804 ms | — | 2.17× | — | — |
| `amberpack.encode_record` | 4KiB-duplicate | 1 | 1 | 4.002 us | — | 4.134 us | — | 0.97× | — | — |
| `amberpack.encode_record` | 4KiB-random | 1 | 1 | 9.786 us | — | 4.130 us | — | 2.37× | — | — |
| `amberpack.encode_record` | 4KiB-text | 1 | 1 | 27.117 us | — | 13.649 us | — | 1.99× | — | — |
| `amberpack.encode_record` | empty | 1 | 1 | 70.6 ns | — | 280.5 ns | — | 0.25× | — | — |
| `amberpack.encode_record` | large-8MiB-duplicate | 1 | 1 | 14.042 ms | — | 5.106 ms | — | 2.75× | — | — |
| `amberpack.encode_record` | large-8MiB-random | 1 | 1 | 6.098 ms | — | 6.229 ms | — | 0.98× | — | — |
| `amberpack.encode_record` | large-8MiB-text | 1 | 1 | 26.298 ms | — | 14.160 ms | — | 1.86× | — | — |
| `amberpack.encode_record` | tiny-64B-duplicate | 1 | 1 | 503.3 ns | — | 597.3 ns | — | 0.84× | — | — |
| `amberpack.encode_record` | tiny-64B-random | 1 | 1 | 562.9 ns | — | 628.5 ns | — | 0.90× | — | — |
| `amberpack.encode_record` | tiny-64B-text | 1 | 1 | 1.152 us | — | 1.172 us | — | 0.98× | — | — |
| `amberpack.parse_record` | corrupt-crc/producer-go | 1 | 1 | 159.6 ns | — | 105.2 ns | — | 1.52× | — | — |
| `amberpack.parse_record` | corrupt-crc/producer-rust | 1 | 1 | 150.9 ns | — | 101.9 ns | — | 1.48× | — | — |
| `amberpack.parse_record` | mixed-records/producer-go | 1 | 1 | 2.209 us | — | 2.535 us | — | 0.87× | — | — |
| `amberpack.parse_record` | mixed-records/producer-rust | 1 | 1 | 2.147 us | — | 2.630 us | — | 0.82× | — | — |
| `amberpack.reader_all` | mixed-objects/producer-go | 1 | 1 | 17.865 us | — | 15.391 us | — | 1.16× | — | — |
| `amberpack.reader_all` | mixed-objects/producer-rust | 1 | 1 | 17.034 us | — | 13.715 us | — | 1.24× | — | — |
| `amberpack.reader_all` | truncated-stream/producer-go | 1 | 1 | 968.750 us | — | 426.293 us | — | 2.27× | — | — |
| `amberpack.reader_all` | truncated-stream/producer-rust | 1 | 1 | 551.543 us | — | 402.468 us | — | 1.37× | — | — |
| `amberpack.reader_records` | mixed-objects/producer-go | 1 | 1 | 2.595 us | — | 2.850 us | — | 0.91× | — | — |
| `amberpack.reader_records` | mixed-objects/producer-rust | 1 | 1 | 1.803 us | — | 2.841 us | — | 0.63× | — | — |
| `amberpack.writer_add` | mixed-objects | 1 | 1 | 92.972 us | — | 40.904 us | — | 2.27× | — | — |
| `amberpack.writer_add_record` | pre-encoded-records/producer-go | 1 | 1 | 147.5 ns | — | 18.9 ns | — | 7.79× | — | — |
| `amberpack.writer_add_record` | pre-encoded-records/producer-rust | 1 | 1 | 142.0 ns | — | 13.5 ns | — | 10.55× | — | — |

### `cbor`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `cbor.decode_xattrs` | xattrs-3 | 1 | 1 | 645.8 ns | — | 195.7 ns | — | 3.30× | — | — |
| `cbor.decode_xattrs` | xattrs-64 | 1 | 1 | 9.379 us | — | 5.599 us | — | 1.67× | — | — |
| `cbor.encode_xattrs` | xattrs-3 | 1 | 1 | 486.2 ns | — | 274.1 ns | — | 1.77× | — | — |
| `cbor.encode_xattrs` | xattrs-64 | 1 | 1 | 25.734 us | — | 4.673 us | — | 5.51× | — | — |

### `chunkers`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `chunkers.item_chunker` | is_boundary/bits-7 | 1 | 1 | 78.5 ns | — | 30.7 ns | — | 2.56× | — | — |
| `chunkers.new_item_chunker` | bits-4..12 | 1 | 1 | 13.1 ns | — | 12.9 ns | — | 1.02× | — | — |
| `chunkers.split_bytes` | compressible/4-16-64KiB | 1 | 1 | 54.192 us | — | 42.344 us | — | 1.28× | — | — |
| `chunkers.split_bytes` | compressible/default-sizes | 1 | 1 | 1.338 ms | — | 666.937 us | — | 2.01× | — | — |
| `chunkers.split_bytes` | random/default-sizes | 1 | 1 | 51.397 us | — | 46.135 us | — | 1.11× | — | — |
| `chunkers.split_bytes` | tiny-64KiB/default-sizes | 1 | 1 | 50.727 us | — | 22.383 us | — | 2.27× | — | — |

### `fstree`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `fstree.check_complete` | incomplete/jobs-1 | 1 | 1 | 52.320 us | — | 15.129 us | — | 3.46× | — | — |
| `fstree.check_complete` | widest/jobs-1 | 1 | 1 | 794.2 ns | — | 355.4 ns | — | 2.24× | — | — |
| `fstree.check_complete` | widest/jobs-N | 4 | 1 | 448.2 ns | — | 168.0 ns | — | 2.67× | — | — |
| `fstree.check_complete` | width-16/jobs-1 | 1 | 1 | 1.899 us | — | 580.5 ns | — | 3.27× | — | — |
| `fstree.check_complete` | width-256/jobs-1 | 1 | 1 | 762.5 ns | — | 361.8 ns | — | 2.11× | — | — |
| `fstree.check_complete` | width-40000/jobs-1 | 1 | 1 | 741.0 ns | — | 272.2 ns | — | 2.72× | — | — |
| `fstree.check_complete` | width-4096/jobs-1 | 1 | 1 | 662.9 ns | — | 314.8 ns | — | 2.11× | — | — |
| `fstree.child_keys` | dir-leaf-128 | 1 | 1 | 47.399 us | — | 29.508 us | — | 1.61× | — | — |
| `fstree.child_keys` | dir-node-128 | 1 | 1 | 21.533 us | — | 14.652 us | — | 1.47× | — | — |
| `fstree.child_keys` | file-node-1024 | 1 | 1 | 86.017 us | — | 72.423 us | — | 1.19× | — | — |
| `fstree.collect_entries` | width-16 | 1 | 1 | 679.4 ns | — | 735.1 ns | — | 0.92× | — | — |
| `fstree.collect_entries` | width-256 | 1 | 1 | 392.1 ns | — | 216.0 ns | — | 1.81× | — | — |
| `fstree.collect_entries` | width-40000 | 1 | 1 | 633.2 ns | — | 217.3 ns | — | 2.91× | — | — |
| `fstree.collect_entries` | width-4096 | 1 | 1 | 367.4 ns | — | 215.4 ns | — | 1.71× | — | — |
| `fstree.decode_dir_leaf` | entries-1024 | 1 | 1 | 365.8 ns | — | 213.4 ns | — | 1.71× | — | — |
| `fstree.decode_dir_leaf` | entries-128 | 1 | 1 | 361.4 ns | — | 212.4 ns | — | 1.70× | — | — |
| `fstree.decode_dir_leaf` | entries-128-partial-change | 1 | 1 | 383.4 ns | — | 223.3 ns | — | 1.72× | — | — |
| `fstree.decode_dir_leaf` | entries-128-with-xattrs | 1 | 1 | 369.6 ns | — | 221.9 ns | — | 1.67× | — | — |
| `fstree.decode_dir_leaf` | entries-8 | 1 | 1 | 375.6 ns | — | 216.3 ns | — | 1.74× | — | — |
| `fstree.decode_dir_node` | pairs-1024 | 1 | 1 | 175.4 ns | — | 96.9 ns | — | 1.81× | — | — |
| `fstree.decode_dir_node` | pairs-128 | 1 | 1 | 168.0 ns | — | 98.5 ns | — | 1.71× | — | — |
| `fstree.decode_dir_node` | pairs-8 | 1 | 1 | 189.2 ns | — | 87.7 ns | — | 2.16× | — | — |
| `fstree.decode_file_node` | children-1024 | 1 | 1 | 86.2 ns | — | 69.4 ns | — | 1.24× | — | — |
| `fstree.decode_file_node` | children-128 | 1 | 1 | 88.7 ns | — | 67.8 ns | — | 1.31× | — | — |
| `fstree.decode_file_node` | children-65536 | 1 | 1 | 99.9 ns | — | 70.1 ns | — | 1.43× | — | — |
| `fstree.decode_file_node` | children-8 | 1 | 1 | 110.6 ns | — | 67.1 ns | — | 1.65× | — | — |
| `fstree.dir_builder` | entries-16 | 1 | 1 | 2.264 us | — | 557.9 ns | — | 4.06× | — | — |
| `fstree.dir_builder` | entries-256 | 1 | 1 | 592.3 ns | — | 346.3 ns | — | 1.71× | — | — |
| `fstree.dir_builder` | entries-40000 | 1 | 1 | 507.2 ns | — | 320.6 ns | — | 1.58× | — | — |
| `fstree.dir_builder` | entries-4096 | 1 | 1 | 485.1 ns | — | 308.5 ns | — | 1.57× | — | — |
| `fstree.encode_blob` | 1MiB-duplicate | 1 | 1 | 218.302 us | — | 137.688 us | — | 1.59× | — | — |
| `fstree.encode_blob` | 1MiB-random | 1 | 1 | 246.140 us | — | 137.152 us | — | 1.79× | — | — |
| `fstree.encode_blob` | 1MiB-text | 1 | 1 | 262.411 us | — | 152.752 us | — | 1.72× | — | — |
| `fstree.encode_blob` | 4KiB-duplicate | 1 | 1 | 1.827 us | — | 1.137 us | — | 1.61× | — | — |
| `fstree.encode_blob` | 4KiB-random | 1 | 1 | 1.959 us | — | 1.137 us | — | 1.72× | — | — |
| `fstree.encode_blob` | 4KiB-text | 1 | 1 | 2.130 us | — | 1.150 us | — | 1.85× | — | — |
| `fstree.encode_blob` | empty | 1 | 1 | 126.1 ns | — | 90.8 ns | — | 1.39× | — | — |
| `fstree.encode_blob` | large-8MiB-duplicate | 1 | 1 | 1.755 ms | — | 1.150 ms | — | 1.53× | — | — |
| `fstree.encode_blob` | large-8MiB-random | 1 | 1 | 1.923 ms | — | 1.091 ms | — | 1.76× | — | — |
| `fstree.encode_blob` | large-8MiB-text | 1 | 1 | 1.930 ms | — | 1.092 ms | — | 1.77× | — | — |
| `fstree.encode_blob` | tiny-64B-duplicate | 1 | 1 | 136.0 ns | — | 110.6 ns | — | 1.23× | — | — |
| `fstree.encode_blob` | tiny-64B-random | 1 | 1 | 135.7 ns | — | 110.4 ns | — | 1.23× | — | — |
| `fstree.encode_blob` | tiny-64B-text | 1 | 1 | 137.2 ns | — | 111.4 ns | — | 1.23× | — | — |
| `fstree.encode_dir_leaf` | entries-1024 | 1 | 1 | 159.5 ns | — | 46.7 ns | — | 3.42× | — | — |
| `fstree.encode_dir_leaf` | entries-128 | 1 | 1 | 164.9 ns | — | 58.9 ns | — | 2.80× | — | — |
| `fstree.encode_dir_leaf` | entries-128-partial-change | 1 | 1 | 177.1 ns | — | 66.4 ns | — | 2.67× | — | — |
| `fstree.encode_dir_leaf` | entries-128-with-xattrs | 1 | 1 | 174.5 ns | — | 68.6 ns | — | 2.54× | — | — |
| `fstree.encode_dir_leaf` | entries-8 | 1 | 1 | 248.9 ns | — | 132.2 ns | — | 1.88× | — | — |
| `fstree.encode_dir_node` | pairs-1024 | 1 | 1 | 67.6 ns | — | 30.9 ns | — | 2.19× | — | — |
| `fstree.encode_dir_node` | pairs-128 | 1 | 1 | 71.3 ns | — | 43.0 ns | — | 1.66× | — | — |
| `fstree.encode_dir_node` | pairs-8 | 1 | 1 | 136.7 ns | — | 90.1 ns | — | 1.52× | — | — |
| `fstree.encode_file_node` | children-1024 | 1 | 1 | 39.0 ns | — | 13.3 ns | — | 2.94× | — | — |
| `fstree.encode_file_node` | children-128 | 1 | 1 | 59.0 ns | — | 19.2 ns | — | 3.08× | — | — |
| `fstree.encode_file_node` | children-65536 | 1 | 1 | 54.6 ns | — | 12.5 ns | — | 4.38× | — | — |
| `fstree.encode_file_node` | children-8 | 1 | 1 | 117.6 ns | — | 47.0 ns | — | 2.50× | — | — |
| `fstree.encode_xattr_set` | xattrs-64 | 1 | 1 | 488.8 ns | — | 99.9 ns | — | 4.89× | — | — |
| `fstree.index_builder_file` | children-1024 | 1 | 1 | 136.6 ns | — | 70.9 ns | — | 1.93× | — | — |
| `fstree.index_builder_file` | children-128 | 1 | 1 | 220.9 ns | — | 70.6 ns | — | 3.13× | — | — |
| `fstree.index_builder_file` | children-65536 | 1 | 1 | 128.4 ns | — | 69.3 ns | — | 1.85× | — | — |
| `fstree.index_builder_file` | children-8 | 1 | 1 | 1.325 us | — | 97.8 ns | — | 13.56× | — | — |
| `fstree.list_entries` | widest/first-page-100 | 1 | 1 | 1.386 us | — | 856.5 ns | — | 1.62× | — | — |
| `fstree.list_entries` | width-16/full-paging-100 | 1 | 1 | 617.4 ns | — | 638.8 ns | — | 0.97× | — | — |
| `fstree.list_entries` | width-256/full-paging-100 | 1 | 1 | 692.4 ns | — | 402.8 ns | — | 1.72× | — | — |
| `fstree.list_entries` | width-40000/full-paging-100 | 1 | 1 | 1.357 us | — | 832.1 ns | — | 1.63× | — | — |
| `fstree.list_entries` | width-4096/full-paging-100 | 1 | 1 | 1.151 us | — | 712.3 ns | — | 1.62× | — | — |
| `fstree.lookup_entry` | width-16/hit | 1 | 1 | 5.150 us | — | 2.909 us | — | 1.77× | — | — |
| `fstree.lookup_entry` | width-16/miss | 1 | 1 | 5.567 us | — | 2.909 us | — | 1.91× | — | — |
| `fstree.lookup_entry` | width-256/hit | 1 | 1 | 40.846 us | — | 25.567 us | — | 1.60× | — | — |
| `fstree.lookup_entry` | width-256/miss | 1 | 1 | 711.4 ns | — | 220.6 ns | — | 3.23× | — | — |
| `fstree.lookup_entry` | width-40000/hit | 1 | 1 | 103.642 us | — | 64.280 us | — | 1.61× | — | — |
| `fstree.lookup_entry` | width-40000/miss | 1 | 1 | 736.1 ns | — | 220.3 ns | — | 3.34× | — | — |
| `fstree.lookup_entry` | width-4096/hit | 1 | 1 | 88.838 us | — | 52.784 us | — | 1.68× | — | — |
| `fstree.lookup_entry` | width-4096/miss | 1 | 1 | 3.736 us | — | 2.028 us | — | 1.84× | — | — |
| `fstree.reachable_keys` | deep | auto | 1 | 5.856 us | — | 43.238 us | — | 0.14× | — | — |
| `fstree.reachable_keys` | file-corpus | auto | 1 | 3.873 us | — | 21.058 us | — | 0.18× | — | — |
| `fstree.reachable_keys` | width-16 | auto | 1 | 1.476 us | — | 6.314 us | — | 0.23× | — | — |
| `fstree.reachable_keys` | width-256 | auto | 1 | 485.5 ns | — | 700.2 ns | — | 0.69× | — | — |
| `fstree.reachable_keys` | width-40000 | auto | 1 | 227.6 ns | — | 108.8 ns | — | 2.09× | — | — |
| `fstree.reachable_keys` | width-4096 | auto | 1 | 142.5 ns | — | 143.5 ns | — | 0.99× | — | — |
| `fstree.resolve_entry` | depth-1 | 1 | 1 | 6.068 us | — | 3.390 us | — | 1.79× | — | — |
| `fstree.resolve_entry` | depth-12 | 1 | 1 | 15.842 us | — | 8.208 us | — | 1.93× | — | — |
| `fstree.resolve_entry` | depth-24 | 1 | 1 | 26.880 us | — | 13.564 us | — | 1.98× | — | — |
| `fstree.resolve_entry` | depth-4 | 1 | 1 | 8.793 us | — | 4.663 us | — | 1.89× | — | — |
| `fstree.resolve_path` | depth-1 | 1 | 1 | 907.4 ns | — | 463.8 ns | — | 1.96× | — | — |
| `fstree.resolve_path` | depth-12 | 1 | 1 | 10.383 us | — | 5.249 us | — | 1.98× | — | — |
| `fstree.resolve_path` | depth-24 | 1 | 1 | 20.649 us | — | 10.567 us | — | 1.95× | — | — |
| `fstree.resolve_path` | depth-4 | 1 | 1 | 3.445 us | — | 1.818 us | — | 1.89× | — | — |
| `fstree.write_content` | file-corpus | 1 | 1 | 929.2 ns | — | 36.547 us | — | 0.03× | — | — |

### `gc`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `gc.close` | idle | 4 | 1 | 290.0 ns | — | 1.152 us | — | 0.25× | — | — |
| `gc.open` | populated | 4 | 1 | 45.056 us | — | 31.410 us | — | 1.43× | — | — |
| `gc.prepare_ref` | missing-root | 4 | 1 | 2.398 us | — | 143.4 ns | — | 16.72× | — | — |
| `gc.prepare_ref` | tree-root/abort | 4 | 1 | 423.248 us | — | 420.513 us | — | 1.01× | — | — |
| `gc.prepare_ref` | tree-root/commit | 4 | 1 | 343.696 us | — | 375.056 us | — | 0.92× | — | — |
| `gc.release_ref` | batch | 4 | 1 | 6.5 ns | — | 0.9 ns | — | 7.44× | — | — |
| `gc.run` | nothing-to-reclaim | 4 | 1 | 223.097 us | — | 181.607 us | — | 1.23× | — | — |
| `gc.run` | reclaimable-packs | 4 | 1 | 1.003 ms | — | 1.013 ms | — | 0.99× | — | — |
| `gc.status` | mark+score | 4 | 1 | 352.944 us | — | 214.660 us | — | 1.64× | — | — |
| `gc.why` | live-root | 4 | 1 | 45.687 us | — | 5.590 us | — | 8.17× | — | — |
| `gc.wipe` | store-reset | 4 | 1 | 463.165 us | — | 430.993 us | — | 1.07× | — | — |

### `inbox`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `inbox.close` | drained | 4 | 1 | 648.609 us | — | 1.614 ms | — | 0.40× | — | — |
| `inbox.commit` | packs/duplicate | 4 | 1 | 20.510 us | — | 21.283 us | — | 0.96× | — | — |
| `inbox.discard` | packs | 4 | 1 | 20.138 us | — | 18.736 us | — | 1.07× | — | — |
| `inbox.drain` | packs/new | 4 | 1 | 318.094 us | — | 281.260 us | — | 1.13× | — | — |
| `inbox.open` | empty | 4 | 1 | 65.585 us | — | 84.231 us | — | 0.78× | — | — |
| `inbox.open` | sweeps-staged-tmp-files | 4 | 1 | 22.858 us | — | 27.657 us | — | 0.83× | — | — |
| `inbox.stage` | packs | 4 | 1 | 199.155 us | — | 183.065 us | — | 1.09× | — | — |

### `ingest`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `ingest.dir` | single-file/jobs-1 | 1 | 1 | 2.738 ms | — | 2.163 ms | — | 1.27× | — | — |
| `ingest.dir` | tree/fresh-store/jobs-1 | 1 | 1 | 222.121 us | — | 43.492 us | — | 5.11× | — | — |
| `ingest.dir` | tree/fresh-store/jobs-N | 4 | 1 | 103.986 us | — | 29.694 us | — | 3.50× | — | — |
| `ingest.dir` | tree/incremental-change/jobs-1 | 1 | 1 | 64.412 us | — | 20.927 us | — | 3.08× | — | — |
| `ingest.dir` | tree/incremental-change/jobs-N | 4 | 1 | 76.084 us | — | 19.856 us | — | 3.83× | — | — |
| `ingest.dir` | tree/unchanged-repeat/jobs-1 | 1 | 1 | 82.117 us | — | 24.786 us | — | 3.31× | — | — |
| `ingest.dir` | tree/unchanged-repeat/jobs-N | 4 | 1 | 61.240 us | — | 18.800 us | — | 3.26× | — | — |
| `ingest.objects` | tree/jobs-1 | 1 | 1 | 75.703 us | — | 22.607 us | — | 3.35× | — | — |
| `ingest.objects` | tree/jobs-N | 4 | 1 | 56.080 us | — | 19.279 us | — | 2.91× | — | — |
| `ingest.scan` | filtered/jobs-1 | 1 | 1 | 2.693 us | — | 2.927 us | — | 0.92× | — | — |
| `ingest.scan` | filtered/jobs-N | 4 | 1 | 1.410 us | — | 3.132 us | — | 0.45× | — | — |
| `ingest.scan` | unfiltered/jobs-1 | 1 | 1 | 1.428 us | — | 1.407 us | — | 1.01× | — | — |

### `key`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `key.accessors` | type+length+length_size+hash | 1 | 1 | 13.4 ns | — | 9.2 ns | — | 1.45× | — | — |
| `key.new` | 1MiB-duplicate | 1 | 1 | 219.595 us | — | 175.290 us | — | 1.25× | — | — |
| `key.new` | 1MiB-random | 1 | 1 | 257.923 us | — | 171.778 us | — | 1.50× | — | — |
| `key.new` | 1MiB-text | 1 | 1 | 249.617 us | — | 172.810 us | — | 1.44× | — | — |
| `key.new` | 4KiB-duplicate | 1 | 1 | 1.781 us | — | 1.329 us | — | 1.34× | — | — |
| `key.new` | 4KiB-random | 1 | 1 | 1.890 us | — | 1.330 us | — | 1.42× | — | — |
| `key.new` | 4KiB-text | 1 | 1 | 2.087 us | — | 1.319 us | — | 1.58× | — | — |
| `key.new` | empty | 1 | 1 | 113.2 ns | — | 79.1 ns | — | 1.43× | — | — |
| `key.new` | large-8MiB-duplicate | 1 | 1 | 1.927 ms | — | 1.404 ms | — | 1.37× | — | — |
| `key.new` | large-8MiB-random | 1 | 1 | 1.921 ms | — | 1.338 ms | — | 1.44× | — | — |
| `key.new` | large-8MiB-text | 1 | 1 | 1.919 ms | — | 1.481 ms | — | 1.30× | — | — |
| `key.new` | tiny-64B-duplicate | 1 | 1 | 108.2 ns | — | 84.0 ns | — | 1.29× | — | — |
| `key.new` | tiny-64B-random | 1 | 1 | 106.8 ns | — | 86.5 ns | — | 1.23× | — | — |
| `key.new` | tiny-64B-text | 1 | 1 | 109.0 ns | — | 86.5 ns | — | 1.26× | — | — |
| `key.new_from_hash` | batch | 1 | 1 | 43.0 ns | — | 36.1 ns | — | 1.19× | — | — |
| `key.parse` | canonical | 1 | 1 | 39.2 ns | — | 27.1 ns | — | 1.45× | — | — |
| `key.parse` | malformed | 1 | 1 | 66.2 ns | — | 1.0 ns | — | 65.73× | — | — |
| `key.string` | hex | 1 | 1 | 94.4 ns | — | 464.4 ns | — | 0.20× | — | — |
| `key.type_string` | names | 1 | 1 | 14.1 ns | — | 15.7 ns | — | 0.90× | — | — |
| `key.validate` | canonical | 1 | 1 | 7.4 ns | — | 1.2 ns | — | 6.38× | — | — |

### `packstore`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `packstore.append_record` | pre-encoded+sync | 1 | 1 | 12.061 us | — | 11.376 us | — | 1.06× | — | — |
| `packstore.barrier` | begin+observe+abort | 1 | 1 | 43.9 ns | — | 40.7 ns | — | 1.08× | — | — |
| `packstore.close` | populated | 1 | 1 | 368.484 us | — | 252.442 us | — | 1.46× | — | — |
| `packstore.compact` | 90-percent-dead | 1 | 1 | 843.6 ns | — | 751.1 ns | — | 1.12× | — | — |
| `packstore.compact` | nothing-dead | 1 | 1 | 317.7 ns | — | 226.4 ns | — | 1.40× | — | — |
| `packstore.get` | hit | 1 | 1 | 7.986 us | — | 5.064 us | — | 1.58× | — | — |
| `packstore.get` | miss | 1 | 1 | 53.9 ns | — | 42.3 ns | — | 1.27× | — | — |
| `packstore.get_record` | hit | 1 | 1 | 502.9 ns | — | 312.6 ns | — | 1.61× | — | — |
| `packstore.has` | hit | 1 | 1 | 61.0 ns | — | 67.9 ns | — | 0.90× | — | — |
| `packstore.has` | miss | 1 | 1 | 29.0 ns | — | 28.3 ns | — | 1.02× | — | — |
| `packstore.has_outside` | sealed-segment | 1 | 1 | 46.0 ns | — | 37.2 ns | — | 1.24× | — | — |
| `packstore.liveness` | tenth-live | 1 | 1 | 32.101 us | — | 30.558 us | — | 1.05× | — | — |
| `packstore.mark_set_contains` | all-keys | 1 | 1 | 92.7 ns | — | 86.3 ns | — | 1.07× | — | — |
| `packstore.mark_set_mark` | all-keys | 1 | 1 | 61.0 ns | — | 45.8 ns | — | 1.33× | — | — |
| `packstore.missing` | half-present | 1 | 1 | 105.4 ns | — | 503.4 ns | — | 0.21× | — | — |
| `packstore.new_mark_set` | snapshot | 1 | 1 | 19.247 us | — | 12.349 us | — | 1.56× | — | — |
| `packstore.oldest_inflight_write` | idle | 1 | 1 | 14.0 ns | — | 3.2 ns | — | 4.36× | — | — |
| `packstore.open` | empty | 1 | 1 | 23.124 us | — | 13.787 us | — | 1.68× | — | — |
| `packstore.open` | populated-reopen | 1 | 1 | 209.510 us | — | 253.104 us | — | 0.83× | — | — |
| `packstore.put` | 1MiB-duplicate/sync-off | 1 | 1 | 723.622 us | — | 275.411 us | — | 2.63× | — | — |
| `packstore.put` | 1MiB-random/sync-off | 1 | 1 | 1.080 ms | — | 513.962 us | — | 2.10× | — | — |
| `packstore.put` | 1MiB-text/sync-off | 1 | 1 | 4.007 ms | — | 1.964 ms | — | 2.04× | — | — |
| `packstore.put` | 4KiB-duplicate/sync-off | 1 | 1 | 5.468 us | — | 3.968 us | — | 1.38× | — | — |
| `packstore.put` | 4KiB-random/already-present | 1 | 1 | 136.8 ns | — | 85.2 ns | — | 1.61× | — | — |
| `packstore.put` | 4KiB-random/sync-off | 1 | 1 | 17.482 us | — | 10.474 us | — | 1.67× | — | — |
| `packstore.put` | 4KiB-random/sync-on | 1 | 1 | 96.929 us | — | 91.706 us | — | 1.06× | — | — |
| `packstore.put` | 4KiB-text/sync-off | 1 | 1 | 54.895 us | — | 19.493 us | — | 2.82× | — | — |
| `packstore.put` | empty/sync-off | 1 | 1 | 263.3 ns | — | 215.6 ns | — | 1.22× | — | — |
| `packstore.put` | large-8MiB-duplicate/sync-off | 1 | 1 | 8.598 ms | — | 2.604 ms | — | 3.30× | — | — |
| `packstore.put` | large-8MiB-random/sync-off | 1 | 1 | 8.768 ms | — | 6.965 ms | — | 1.26× | — | — |
| `packstore.put` | large-8MiB-text/sync-off | 1 | 1 | 26.966 ms | — | 14.796 ms | — | 1.82× | — | — |
| `packstore.put` | tiny-64B-duplicate/sync-off | 1 | 1 | 162.0 ns | — | 150.1 ns | — | 1.08× | — | — |
| `packstore.put` | tiny-64B-random/sync-off | 1 | 1 | 2.315 us | — | 1.763 us | — | 1.31× | — | — |
| `packstore.put` | tiny-64B-text/sync-off | 1 | 1 | 2.540 us | — | 2.340 us | — | 1.09× | — | — |
| `packstore.put_verified` | already-intact | 1 | 1 | 81.305 us | — | 52.296 us | — | 1.55× | — | — |
| `packstore.record` | by-location | 1 | 1 | 832.3 ns | — | 824.3 ns | — | 1.01× | — | — |
| `packstore.remove` | one-sealed-segment | 1 | 1 | 105.471 us | — | 96.865 us | — | 1.09× | — | — |
| `packstore.scan_index` | all-segments | 1 | 1 | 30.2 ns | — | 23.2 ns | — | 1.30× | — | — |
| `packstore.segments` | list | 1 | 1 | 2.791 us | — | 2.089 us | — | 1.34× | — | — |
| `packstore.sort_by_location` | scattered | 1 | 1 | 180.9 ns | — | 103.0 ns | — | 1.76× | — | — |
| `packstore.stored_size` | hit | 1 | 1 | 72.4 ns | — | 55.7 ns | — | 1.30× | — | — |
| `packstore.verify` | full-scrub | 1 | 1 | 7.755 us | — | 4.829 us | — | 1.61× | — | — |
| `packstore.wipe` | populated | 1 | 1 | 245.238 us | — | 225.431 us | — | 1.09× | — | — |
| `packstore.write_batch` | mixed-objects | 1 | 1 | 34.314 us | — | 22.415 us | — | 1.53× | — | — |
| `packstore.write_parallel` | duplicate-stream/writers-N | 4 | 1 | 273.1 ns | — | 858.6 ns | — | 0.32× | — | — |
| `packstore.write_parallel` | mixed-objects/writers-1/verify-false | 1 | 1 | 34.047 us | — | 22.613 us | — | 1.51× | — | — |
| `packstore.write_parallel` | mixed-objects/writers-1/verify-true | 1 | 1 | 35.594 us | — | 24.095 us | — | 1.48× | — | — |
| `packstore.write_parallel` | mixed-objects/writers-N/verify-false | 4 | 1 | 11.675 us | — | 7.197 us | — | 1.62× | — | — |
| `packstore.write_parallel` | mixed-objects/writers-N/verify-true | 4 | 1 | 12.503 us | — | 8.110 us | — | 1.54× | — | — |

### `reference`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `reference.decode` | signed | 1 | 1 | 862.6 ns | — | 441.8 ns | — | 1.95× | — | — |
| `reference.encode` | signed | 1 | 1 | 253.6 ns | — | 171.8 ns | — | 1.48× | — | — |
| `reference.signature_payload` | signed | 1 | 1 | 249.7 ns | — | 166.6 ns | — | 1.50× | — | — |
| `reference.validate_name` | valid | 1 | 1 | 66.5 ns | — | 33.3 ns | — | 2.00× | — | — |
| `reference.validate_user` | valid | 1 | 1 | 71.5 ns | — | 96.7 ns | — | 0.74× | — | — |

### `refstore`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `refstore.all` | records | 1 | 1 | 491.6 ns | — | 189.4 ns | — | 2.60× | — | — |
| `refstore.close` | populated | 1 | 1 | 143.855 us | — | 959.332 us | — | 0.15× | — | — |
| `refstore.delete` | records | 1 | 1 | 1.669 us | — | 36.458 us | — | 0.05× | — | — |
| `refstore.get` | hit | 1 | 1 | 1.104 us | — | 436.4 ns | — | 2.53× | — | — |
| `refstore.get` | miss | 1 | 1 | 221.0 ns | — | 322.1 ns | — | 0.69× | — | — |
| `refstore.open` | empty | 1 | 1 | 722.390 us | — | 1.241 ms | — | 0.58× | — | — |
| `refstore.open` | populated-reopen | 1 | 1 | 1.040 ms | — | 756.205 us | — | 1.38× | — | — |
| `refstore.put` | records/sync-false | 1 | 1 | 381.5 ns | — | 40.186 us | — | 0.01× | — | — |
| `refstore.put` | records/sync-true | 1 | 1 | 60.817 us | — | 39.554 us | — | 1.54× | — | — |
| `refstore.put_batch` | records/sync-false | 1 | 1 | 100.6 ns | — | 1.190 us | — | 0.08× | — | — |
| `refstore.wipe` | records | 1 | 1 | 137.022 us | — | 354.938 us | — | 0.39× | — | — |

### `tar`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `tarexport.write` | fixture-tree | 1 | 1 | 2.593 us | — | 3.438 us | — | 0.75× | — | — |
| `tarextract.extract` | fixture-tree | 1 | 1 | 40.966 us | — | 36.819 us | — | 1.11× | — | — |

## Rates

Each operation in the unit it is actually in. `ops/s` counts the core operations
the case declares; `entries/s` and `objects/s` count the tree entries and stored
objects its dimensions record, both computed outside the measured interval. The
byte column names what its bytes are — a metadata walk that never opens a file
body is measured in *logical bytes scanned*, not in bandwidth.

| Operation | Workload | Bytes counted | Go | Rust | Go ops/s | Rust ops/s | Go entries/s | Rust entries/s |
|---|---|---|---:|---:|---:|---:|---:|---:|
| `amberignore.descend` | 8-subdirs | — | — | — | 188.29 k/s | 304.81 k/s | — | — |
| `amberignore.ignored` | mixed-names | — | — | — | 4.86 M/s | 8.97 M/s | — | — |
| `amberignore.ignored` | nil-matcher | — | — | — | 183.84 M/s | 4.63 G/s | — | — |
| `amberignore.root` | load-root | — | — | — | 131.84 k/s | 224.59 k/s | — | — |
| `amberpack.decode_payload` | mixed-records/producer-go | encoded bytes | 1192.3 MiB/s | 1862.3 MiB/s | 62.08 k/s | 96.97 k/s | — | — |
| `amberpack.decode_payload` | mixed-records/producer-rust | encoded bytes | 1319.8 MiB/s | 2037.4 MiB/s | 68.83 k/s | 106.25 k/s | — | — |
| `amberpack.encode_record` | 1MiB-duplicate | payload bytes | 1656.0 MiB/s | 3502.2 MiB/s | 1.66 k/s | 3.50 k/s | — | — |
| `amberpack.encode_record` | 1MiB-random | payload bytes | 1930.7 MiB/s | 3593.9 MiB/s | 1.93 k/s | 3.59 k/s | — | — |
| `amberpack.encode_record` | 1MiB-text | payload bytes | 255.7 MiB/s | 554.3 MiB/s | 256 /s | 554 /s | — | — |
| `amberpack.encode_record` | 4KiB-duplicate | payload bytes | 976.0 MiB/s | 944.9 MiB/s | 249.85 k/s | 241.91 k/s | — | — |
| `amberpack.encode_record` | 4KiB-random | payload bytes | 399.2 MiB/s | 945.9 MiB/s | 102.19 k/s | 242.15 k/s | — | — |
| `amberpack.encode_record` | 4KiB-text | payload bytes | 144.0 MiB/s | 286.2 MiB/s | 36.88 k/s | 73.26 k/s | — | — |
| `amberpack.encode_record` | empty | payload bytes | — | — | 14.16 M/s | 3.57 M/s | — | — |
| `amberpack.encode_record` | large-8MiB-duplicate | payload bytes | 569.7 MiB/s | 1566.7 MiB/s | 71 /s | 196 /s | — | — |
| `amberpack.encode_record` | large-8MiB-random | payload bytes | 1311.9 MiB/s | 1284.2 MiB/s | 164 /s | 161 /s | — | — |
| `amberpack.encode_record` | large-8MiB-text | payload bytes | 304.2 MiB/s | 565.0 MiB/s | 38 /s | 71 /s | — | — |
| `amberpack.encode_record` | tiny-64B-duplicate | payload bytes | 121.3 MiB/s | 102.2 MiB/s | 1.99 M/s | 1.67 M/s | — | — |
| `amberpack.encode_record` | tiny-64B-random | payload bytes | 108.4 MiB/s | 97.1 MiB/s | 1.78 M/s | 1.59 M/s | — | — |
| `amberpack.encode_record` | tiny-64B-text | payload bytes | 53.0 MiB/s | 52.1 MiB/s | 867.95 k/s | 853.04 k/s | — | — |
| `amberpack.parse_record` | corrupt-crc/producer-go | — | — | — | 6.27 M/s | 9.51 M/s | — | — |
| `amberpack.parse_record` | corrupt-crc/producer-rust | — | — | — | 6.63 M/s | 9.81 M/s | — | — |
| `amberpack.parse_record` | mixed-records/producer-go | encoded bytes | 8695.2 MiB/s | 7576.2 MiB/s | 452.74 k/s | 394.48 k/s | — | — |
| `amberpack.parse_record` | mixed-records/producer-rust | encoded bytes | 8931.2 MiB/s | 7289.8 MiB/s | 465.78 k/s | 380.18 k/s | — | — |
| `amberpack.reader_all` | mixed-objects/producer-go | encoded bytes | 1075.0 MiB/s | 1247.8 MiB/s | 55.98 k/s | 64.97 k/s | — | — |
| `amberpack.reader_all` | mixed-objects/producer-rust | encoded bytes | 1125.7 MiB/s | 1398.1 MiB/s | 58.71 k/s | 72.91 k/s | — | — |
| `amberpack.reader_all` | truncated-stream/producer-go | — | — | — | 1.03 k/s | 2.35 k/s | — | — |
| `amberpack.reader_all` | truncated-stream/producer-rust | — | — | — | 1.81 k/s | 2.48 k/s | — | — |
| `amberpack.reader_records` | mixed-objects/producer-go | encoded bytes | 7400.2 MiB/s | 6738.8 MiB/s | 385.31 k/s | 350.88 k/s | — | — |
| `amberpack.reader_records` | mixed-objects/producer-rust | encoded bytes | 10635.9 MiB/s | 6748.7 MiB/s | 554.69 k/s | 351.96 k/s | — | — |
| `amberpack.writer_add` | mixed-objects | payload bytes | 347.9 MiB/s | 790.8 MiB/s | 10.76 k/s | 24.45 k/s | — | — |
| `amberpack.writer_add_record` | pre-encoded-records/producer-go | encoded bytes | 130234.9 MiB/s | 1014156.3 MiB/s | 6.78 M/s | 52.81 M/s | — | — |
| `amberpack.writer_add_record` | pre-encoded-records/producer-rust | encoded bytes | 135047.4 MiB/s | 1425291.2 MiB/s | 7.04 M/s | 74.33 M/s | — | — |
| `cbor.decode_xattrs` | xattrs-3 | encoded bytes | 104.9 MiB/s | 346.0 MiB/s | 1.55 M/s | 5.11 M/s | — | — |
| `cbor.decode_xattrs` | xattrs-64 | encoded bytes | 449.2 MiB/s | 752.5 MiB/s | 106.62 k/s | 178.59 k/s | — | — |
| `cbor.encode_xattrs` | xattrs-3 | encoded bytes | 139.3 MiB/s | 247.0 MiB/s | 2.06 M/s | 3.65 M/s | — | — |
| `cbor.encode_xattrs` | xattrs-64 | encoded bytes | 163.7 MiB/s | 901.7 MiB/s | 38.86 k/s | 214.02 k/s | — | — |
| `chunkers.item_chunker` | is_boundary/bits-7 | — | — | — | 12.73 M/s | 32.56 M/s | — | — |
| `chunkers.new_item_chunker` | bits-4..12 | — | — | — | 76.32 M/s | 77.64 M/s | — | — |
| `chunkers.split_bytes` | compressible/4-16-64KiB | payload bytes | 1153.3 MiB/s | 1476.0 MiB/s | 18.45 k/s | 23.62 k/s | — | — |
| `chunkers.split_bytes` | compressible/default-sizes | payload bytes | 747.6 MiB/s | 1499.4 MiB/s | 748 /s | 1.50 k/s | — | — |
| `chunkers.split_bytes` | random/default-sizes | payload bytes | 1441.2 MiB/s | 1605.6 MiB/s | 19.46 k/s | 21.68 k/s | — | — |
| `chunkers.split_bytes` | tiny-64KiB/default-sizes | payload bytes | 1232.1 MiB/s | 2792.3 MiB/s | 19.71 k/s | 44.68 k/s | — | — |
| `fstree.check_complete` | incomplete/jobs-1 | — | — | — | 19.11 k/s | 66.10 k/s | — | — |
| `fstree.check_complete` | widest/jobs-1 | — | — | — | 1.26 M/s | 2.81 M/s | — | — |
| `fstree.check_complete` | widest/jobs-N | — | — | — | 2.23 M/s | 5.95 M/s | — | — |
| `fstree.check_complete` | width-16/jobs-1 | — | — | — | 526.61 k/s | 1.72 M/s | — | — |
| `fstree.check_complete` | width-256/jobs-1 | — | — | — | 1.31 M/s | 2.76 M/s | — | — |
| `fstree.check_complete` | width-40000/jobs-1 | — | — | — | 1.35 M/s | 3.67 M/s | — | — |
| `fstree.check_complete` | width-4096/jobs-1 | — | — | — | 1.51 M/s | 3.18 M/s | — | — |
| `fstree.child_keys` | dir-leaf-128 | — | — | — | 21.10 k/s | 33.89 k/s | — | — |
| `fstree.child_keys` | dir-node-128 | — | — | — | 46.44 k/s | 68.25 k/s | — | — |
| `fstree.child_keys` | file-node-1024 | — | — | — | 11.63 k/s | 13.81 k/s | — | — |
| `fstree.collect_entries` | width-16 | — | — | — | 1.47 M/s | 1.36 M/s | 1.47 M/s | 1.36 M/s |
| `fstree.collect_entries` | width-256 | — | — | — | 2.55 M/s | 4.63 M/s | 2.55 M/s | 4.63 M/s |
| `fstree.collect_entries` | width-40000 | — | — | — | 1.58 M/s | 4.60 M/s | 1.58 M/s | 4.60 M/s |
| `fstree.collect_entries` | width-4096 | — | — | — | 2.72 M/s | 4.64 M/s | 2.72 M/s | 4.64 M/s |
| `fstree.decode_dir_leaf` | entries-1024 | encoded bytes | 193.0 MiB/s | 330.7 MiB/s | 2.73 M/s | 4.69 M/s | 2.73 M/s | 4.69 M/s |
| `fstree.decode_dir_leaf` | entries-128 | encoded bytes | 195.3 MiB/s | 332.3 MiB/s | 2.77 M/s | 4.71 M/s | 2.77 M/s | 4.71 M/s |
| `fstree.decode_dir_leaf` | entries-128-partial-change | encoded bytes | 206.5 MiB/s | 354.6 MiB/s | 2.61 M/s | 4.48 M/s | 2.61 M/s | 4.48 M/s |
| `fstree.decode_dir_leaf` | entries-128-with-xattrs | encoded bytes | 214.2 MiB/s | 356.8 MiB/s | 2.71 M/s | 4.51 M/s | 2.71 M/s | 4.51 M/s |
| `fstree.decode_dir_leaf` | entries-8 | encoded bytes | 188.2 MiB/s | 326.8 MiB/s | 2.66 M/s | 4.62 M/s | 2.66 M/s | 4.62 M/s |
| `fstree.decode_dir_node` | pairs-1024 | encoded bytes | 271.8 MiB/s | 492.2 MiB/s | 5.70 M/s | 10.32 M/s | 5.70 M/s | 10.32 M/s |
| `fstree.decode_dir_node` | pairs-128 | encoded bytes | 283.9 MiB/s | 484.2 MiB/s | 5.95 M/s | 10.15 M/s | 5.95 M/s | 10.15 M/s |
| `fstree.decode_dir_node` | pairs-8 | encoded bytes | 252.7 MiB/s | 544.9 MiB/s | 5.29 M/s | 11.40 M/s | 5.29 M/s | 11.40 M/s |
| `fstree.decode_file_node` | children-1024 | encoded bytes | 376.0 MiB/s | 467.0 MiB/s | 11.60 M/s | 14.40 M/s | 11.60 M/s | 14.40 M/s |
| `fstree.decode_file_node` | children-128 | encoded bytes | 365.5 MiB/s | 478.5 MiB/s | 11.27 M/s | 14.75 M/s | 11.27 M/s | 14.75 M/s |
| `fstree.decode_file_node` | children-65536 | encoded bytes | 324.5 MiB/s | 462.5 MiB/s | 10.01 M/s | 14.26 M/s | 10.01 M/s | 14.26 M/s |
| `fstree.decode_file_node` | children-8 | encoded bytes | 294.3 MiB/s | 484.7 MiB/s | 9.04 M/s | 14.89 M/s | 9.04 M/s | 14.89 M/s |
| `fstree.dir_builder` | entries-16 | — | — | — | 441.76 k/s | 1.79 M/s | 441.76 k/s | 1.79 M/s |
| `fstree.dir_builder` | entries-256 | — | — | — | 1.69 M/s | 2.89 M/s | 1.69 M/s | 2.89 M/s |
| `fstree.dir_builder` | entries-40000 | — | — | — | 1.97 M/s | 3.12 M/s | 1.97 M/s | 3.12 M/s |
| `fstree.dir_builder` | entries-4096 | — | — | — | 2.06 M/s | 3.24 M/s | 2.06 M/s | 3.24 M/s |
| `fstree.encode_blob` | 1MiB-duplicate | payload bytes | 4580.8 MiB/s | 7262.8 MiB/s | 4.58 k/s | 7.26 k/s | — | — |
| `fstree.encode_blob` | 1MiB-random | payload bytes | 4062.7 MiB/s | 7291.2 MiB/s | 4.06 k/s | 7.29 k/s | — | — |
| `fstree.encode_blob` | 1MiB-text | payload bytes | 3810.8 MiB/s | 6546.6 MiB/s | 3.81 k/s | 6.55 k/s | — | — |
| `fstree.encode_blob` | 4KiB-duplicate | payload bytes | 2138.1 MiB/s | 3436.6 MiB/s | 547.35 k/s | 879.77 k/s | — | — |
| `fstree.encode_blob` | 4KiB-random | payload bytes | 1993.7 MiB/s | 3434.6 MiB/s | 510.40 k/s | 879.25 k/s | — | — |
| `fstree.encode_blob` | 4KiB-text | payload bytes | 1834.1 MiB/s | 3396.3 MiB/s | 469.53 k/s | 869.44 k/s | — | — |
| `fstree.encode_blob` | empty | payload bytes | — | — | 7.93 M/s | 11.02 M/s | — | — |
| `fstree.encode_blob` | large-8MiB-duplicate | payload bytes | 4558.8 MiB/s | 6958.7 MiB/s | 570 /s | 870 /s | — | — |
| `fstree.encode_blob` | large-8MiB-random | payload bytes | 4161.1 MiB/s | 7330.6 MiB/s | 520 /s | 916 /s | — | — |
| `fstree.encode_blob` | large-8MiB-text | payload bytes | 4146.1 MiB/s | 7325.8 MiB/s | 518 /s | 916 /s | — | — |
| `fstree.encode_blob` | tiny-64B-duplicate | payload bytes | 448.8 MiB/s | 551.9 MiB/s | 7.35 M/s | 9.04 M/s | — | — |
| `fstree.encode_blob` | tiny-64B-random | payload bytes | 449.9 MiB/s | 553.0 MiB/s | 7.37 M/s | 9.06 M/s | — | — |
| `fstree.encode_blob` | tiny-64B-text | payload bytes | 444.7 MiB/s | 548.0 MiB/s | 7.29 M/s | 8.98 M/s | — | — |
| `fstree.encode_dir_leaf` | entries-1024 | encoded bytes | 442.5 MiB/s | 1511.8 MiB/s | 6.27 M/s | 21.42 M/s | 6.27 M/s | 21.42 M/s |
| `fstree.encode_dir_leaf` | entries-128 | encoded bytes | 428.0 MiB/s | 1198.1 MiB/s | 6.06 M/s | 16.97 M/s | 6.06 M/s | 16.97 M/s |
| `fstree.encode_dir_leaf` | entries-128-partial-change | encoded bytes | 447.1 MiB/s | 1192.9 MiB/s | 5.65 M/s | 15.07 M/s | 5.65 M/s | 15.07 M/s |
| `fstree.encode_dir_leaf` | entries-128-with-xattrs | encoded bytes | 453.6 MiB/s | 1154.0 MiB/s | 5.73 M/s | 14.58 M/s | 5.73 M/s | 14.58 M/s |
| `fstree.encode_dir_leaf` | entries-8 | encoded bytes | 284.0 MiB/s | 534.9 MiB/s | 4.02 M/s | 7.57 M/s | 4.02 M/s | 7.57 M/s |
| `fstree.encode_dir_node` | pairs-1024 | encoded bytes | 705.2 MiB/s | 1543.1 MiB/s | 14.79 M/s | 32.36 M/s | 14.79 M/s | 32.36 M/s |
| `fstree.encode_dir_node` | pairs-128 | encoded bytes | 669.2 MiB/s | 1109.5 MiB/s | 14.03 M/s | 23.26 M/s | 14.03 M/s | 23.26 M/s |
| `fstree.encode_dir_node` | pairs-8 | encoded bytes | 349.7 MiB/s | 530.8 MiB/s | 7.31 M/s | 11.10 M/s | 7.31 M/s | 11.10 M/s |
| `fstree.encode_file_node` | children-1024 | encoded bytes | 831.1 MiB/s | 2443.6 MiB/s | 25.63 M/s | 75.35 M/s | 25.63 M/s | 75.35 M/s |
| `fstree.encode_file_node` | children-128 | encoded bytes | 549.6 MiB/s | 1690.8 MiB/s | 16.94 M/s | 52.12 M/s | 16.94 M/s | 52.12 M/s |
| `fstree.encode_file_node` | children-65536 | encoded bytes | 594.3 MiB/s | 2601.0 MiB/s | 18.33 M/s | 80.22 M/s | 18.33 M/s | 80.22 M/s |
| `fstree.encode_file_node` | children-8 | encoded bytes | 276.7 MiB/s | 692.7 MiB/s | 8.50 M/s | 21.28 M/s | 8.50 M/s | 21.28 M/s |
| `fstree.encode_xattr_set` | xattrs-64 | — | — | — | 2.05 M/s | 10.01 M/s | — | — |
| `fstree.index_builder_file` | children-1024 | — | — | — | 7.32 M/s | 14.10 M/s | 7.32 M/s | 14.10 M/s |
| `fstree.index_builder_file` | children-128 | — | — | — | 4.53 M/s | 14.16 M/s | 4.53 M/s | 14.16 M/s |
| `fstree.index_builder_file` | children-65536 | — | — | — | 7.79 M/s | 14.43 M/s | 7.79 M/s | 14.43 M/s |
| `fstree.index_builder_file` | children-8 | — | — | — | 754.65 k/s | 10.23 M/s | 754.65 k/s | 10.23 M/s |
| `fstree.list_entries` | widest/first-page-100 | — | — | — | 721.71 k/s | 1.17 M/s | 721.71 k/s | 1.17 M/s |
| `fstree.list_entries` | width-16/full-paging-100 | — | — | — | 1.62 M/s | 1.57 M/s | 1.62 M/s | 1.57 M/s |
| `fstree.list_entries` | width-256/full-paging-100 | — | — | — | 1.44 M/s | 2.48 M/s | 1.44 M/s | 2.48 M/s |
| `fstree.list_entries` | width-40000/full-paging-100 | — | — | — | 736.85 k/s | 1.20 M/s | 736.85 k/s | 1.20 M/s |
| `fstree.list_entries` | width-4096/full-paging-100 | — | — | — | 869.03 k/s | 1.40 M/s | 869.03 k/s | 1.40 M/s |
| `fstree.lookup_entry` | width-16/hit | — | — | — | 194.16 k/s | 343.81 k/s | — | — |
| `fstree.lookup_entry` | width-16/miss | — | — | — | 179.63 k/s | 343.76 k/s | — | — |
| `fstree.lookup_entry` | width-256/hit | — | — | — | 24.48 k/s | 39.11 k/s | — | — |
| `fstree.lookup_entry` | width-256/miss | — | — | — | 1.41 M/s | 4.53 M/s | — | — |
| `fstree.lookup_entry` | width-40000/hit | — | — | — | 9.65 k/s | 15.56 k/s | — | — |
| `fstree.lookup_entry` | width-40000/miss | — | — | — | 1.36 M/s | 4.54 M/s | — | — |
| `fstree.lookup_entry` | width-4096/hit | — | — | — | 11.26 k/s | 18.95 k/s | — | — |
| `fstree.lookup_entry` | width-4096/miss | — | — | — | 267.67 k/s | 493.08 k/s | — | — |
| `fstree.reachable_keys` | deep | — | — | — | 170.77 k/s | 23.13 k/s | — | — |
| `fstree.reachable_keys` | file-corpus | — | — | — | 258.23 k/s | 47.49 k/s | — | — |
| `fstree.reachable_keys` | width-16 | — | — | — | 677.61 k/s | 158.37 k/s | — | — |
| `fstree.reachable_keys` | width-256 | — | — | — | 2.06 M/s | 1.43 M/s | — | — |
| `fstree.reachable_keys` | width-40000 | — | — | — | 4.39 M/s | 9.20 M/s | — | — |
| `fstree.reachable_keys` | width-4096 | — | — | — | 7.02 M/s | 6.97 M/s | — | — |
| `fstree.resolve_entry` | depth-1 | — | — | — | 164.79 k/s | 295.01 k/s | — | — |
| `fstree.resolve_entry` | depth-12 | — | — | — | 63.12 k/s | 121.83 k/s | — | — |
| `fstree.resolve_entry` | depth-24 | — | — | — | 37.20 k/s | 73.73 k/s | — | — |
| `fstree.resolve_entry` | depth-4 | — | — | — | 113.73 k/s | 214.44 k/s | — | — |
| `fstree.resolve_path` | depth-1 | — | — | — | 1.10 M/s | 2.16 M/s | — | — |
| `fstree.resolve_path` | depth-12 | — | — | — | 96.31 k/s | 190.52 k/s | — | — |
| `fstree.resolve_path` | depth-24 | — | — | — | 48.43 k/s | 94.64 k/s | — | — |
| `fstree.resolve_path` | depth-4 | — | — | — | 290.24 k/s | 549.94 k/s | — | — |
| `fstree.write_content` | file-corpus | payload bytes | 1076136.7 MiB/s | 27361.7 MiB/s | 1.08 M/s | 27.36 k/s | — | — |
| `gc.close` | idle | — | — | — | 3.45 M/s | 868.06 k/s | — | — |
| `gc.open` | populated | — | — | — | 22.19 k/s | 31.84 k/s | — | — |
| `gc.prepare_ref` | missing-root | — | — | — | 417.07 k/s | 6.97 M/s | — | — |
| `gc.prepare_ref` | tree-root/abort | — | — | — | 2.36 k/s | 2.38 k/s | — | — |
| `gc.prepare_ref` | tree-root/commit | — | — | — | 2.91 k/s | 2.67 k/s | — | — |
| `gc.release_ref` | batch | — | — | — | 154.33 M/s | 1.15 G/s | — | — |
| `gc.run` | nothing-to-reclaim | — | — | — | 4.48 k/s | 5.51 k/s | — | — |
| `gc.run` | reclaimable-packs | — | — | — | 997 /s | 987 /s | — | — |
| `gc.status` | mark+score | — | — | — | 2.83 k/s | 4.66 k/s | — | — |
| `gc.why` | live-root | — | — | — | 21.89 k/s | 178.89 k/s | — | — |
| `gc.wipe` | store-reset | — | — | — | 2.16 k/s | 2.32 k/s | — | — |
| `inbox.close` | drained | — | — | — | 1.54 k/s | 620 /s | — | — |
| `inbox.commit` | packs/duplicate | — | — | — | 48.76 k/s | 46.99 k/s | — | — |
| `inbox.discard` | packs | — | — | — | 49.66 k/s | 53.37 k/s | — | — |
| `inbox.drain` | packs/new | payload bytes | 844.1 MiB/s | 954.6 MiB/s | 3.14 k/s | 3.56 k/s | — | — |
| `inbox.open` | empty | — | — | — | 15.25 k/s | 11.87 k/s | — | — |
| `inbox.open` | sweeps-staged-tmp-files | — | — | — | 43.75 k/s | 36.16 k/s | — | — |
| `inbox.stage` | packs | payload bytes | 1348.2 MiB/s | 1466.7 MiB/s | 5.02 k/s | 5.46 k/s | — | — |
| `ingest.dir` | single-file/jobs-1 | included payload bytes | 182.6 MiB/s | 231.2 MiB/s | 365 /s | 462 /s | — | — |
| `ingest.dir` | tree/fresh-store/jobs-1 | included payload bytes | 92.2 MiB/s | 471.0 MiB/s | 4.50 k/s | 22.99 k/s | — | — |
| `ingest.dir` | tree/fresh-store/jobs-N | included payload bytes | 197.0 MiB/s | 689.9 MiB/s | 9.62 k/s | 33.68 k/s | — | — |
| `ingest.dir` | tree/incremental-change/jobs-1 | included payload bytes | 312.0 MiB/s | 960.3 MiB/s | 15.53 k/s | 47.78 k/s | — | — |
| `ingest.dir` | tree/incremental-change/jobs-N | included payload bytes | 264.1 MiB/s | 1012.2 MiB/s | 13.14 k/s | 50.36 k/s | — | — |
| `ingest.dir` | tree/unchanged-repeat/jobs-1 | included payload bytes | 249.5 MiB/s | 826.5 MiB/s | 12.18 k/s | 40.34 k/s | — | — |
| `ingest.dir` | tree/unchanged-repeat/jobs-N | included payload bytes | 334.5 MiB/s | 1089.7 MiB/s | 16.33 k/s | 53.19 k/s | — | — |
| `ingest.objects` | tree/jobs-1 | included payload bytes | 270.6 MiB/s | 906.2 MiB/s | 13.21 k/s | 44.23 k/s | — | — |
| `ingest.objects` | tree/jobs-N | included payload bytes | 365.3 MiB/s | 1062.6 MiB/s | 17.83 k/s | 51.87 k/s | — | — |
| `ingest.scan` | filtered/jobs-1 | logical bytes scanned | 7607.1 MiB/s | 6999.3 MiB/s | 371.32 k/s | 341.65 k/s | — | — |
| `ingest.scan` | filtered/jobs-N | logical bytes scanned | 14525.5 MiB/s | 6541.7 MiB/s | 709.02 k/s | 319.31 k/s | — | — |
| `ingest.scan` | unfiltered/jobs-1 | logical bytes scanned | 13604.7 MiB/s | 13806.1 MiB/s | 700.52 k/s | 710.89 k/s | — | — |
| `key.accessors` | type+length+length_size+hash | — | — | — | 74.69 M/s | 108.49 M/s | — | — |
| `key.new` | 1MiB-duplicate | payload bytes | 4553.8 MiB/s | 5704.8 MiB/s | 4.55 k/s | 5.70 k/s | — | — |
| `key.new` | 1MiB-random | payload bytes | 3877.1 MiB/s | 5821.5 MiB/s | 3.88 k/s | 5.82 k/s | — | — |
| `key.new` | 1MiB-text | payload bytes | 4006.1 MiB/s | 5786.7 MiB/s | 4.01 k/s | 5.79 k/s | — | — |
| `key.new` | 4KiB-duplicate | payload bytes | 2192.9 MiB/s | 2940.0 MiB/s | 561.38 k/s | 752.63 k/s | — | — |
| `key.new` | 4KiB-random | payload bytes | 2067.3 MiB/s | 2936.9 MiB/s | 529.24 k/s | 751.84 k/s | — | — |
| `key.new` | 4KiB-text | payload bytes | 1871.6 MiB/s | 2961.9 MiB/s | 479.14 k/s | 758.24 k/s | — | — |
| `key.new` | empty | payload bytes | — | — | 8.84 M/s | 12.64 M/s | — | — |
| `key.new` | large-8MiB-duplicate | payload bytes | 4151.3 MiB/s | 5698.8 MiB/s | 519 /s | 712 /s | — | — |
| `key.new` | large-8MiB-random | payload bytes | 4163.5 MiB/s | 5977.6 MiB/s | 520 /s | 747 /s | — | — |
| `key.new` | large-8MiB-text | payload bytes | 4168.3 MiB/s | 5400.6 MiB/s | 521 /s | 675 /s | — | — |
| `key.new` | tiny-64B-duplicate | payload bytes | 564.2 MiB/s | 726.4 MiB/s | 9.24 M/s | 11.90 M/s | — | — |
| `key.new` | tiny-64B-random | payload bytes | 571.6 MiB/s | 705.3 MiB/s | 9.37 M/s | 11.56 M/s | — | — |
| `key.new` | tiny-64B-text | payload bytes | 560.0 MiB/s | 706.0 MiB/s | 9.17 M/s | 11.57 M/s | — | — |
| `key.new_from_hash` | batch | — | — | — | 23.27 M/s | 27.72 M/s | — | — |
| `key.parse` | canonical | — | — | — | 25.53 M/s | 36.90 M/s | — | — |
| `key.parse` | malformed | — | — | — | 15.10 M/s | 992.25 M/s | — | — |
| `key.string` | hex | — | — | — | 10.59 M/s | 2.15 M/s | — | — |
| `key.type_string` | names | — | — | — | 71.02 M/s | 63.57 M/s | — | — |
| `key.validate` | canonical | — | — | — | 135.78 M/s | 865.80 M/s | — | — |
| `packstore.append_record` | pre-encoded+sync | payload bytes | 2682.0 MiB/s | 2843.6 MiB/s | 82.91 k/s | 87.91 k/s | — | — |
| `packstore.barrier` | begin+observe+abort | — | — | — | 22.76 M/s | 24.58 M/s | — | — |
| `packstore.close` | populated | — | — | — | 2.71 k/s | 3.96 k/s | — | — |
| `packstore.compact` | 90-percent-dead | — | — | — | 1.19 M/s | 1.33 M/s | — | — |
| `packstore.compact` | nothing-dead | — | — | — | 3.15 M/s | 4.42 M/s | — | — |
| `packstore.get` | hit | — | — | — | 125.22 k/s | 197.47 k/s | — | — |
| `packstore.get` | miss | — | — | — | 18.55 M/s | 23.62 M/s | — | — |
| `packstore.get_record` | hit | — | — | — | 1.99 M/s | 3.20 M/s | — | — |
| `packstore.has` | hit | — | — | — | 16.39 M/s | 14.72 M/s | — | — |
| `packstore.has` | miss | — | — | — | 34.53 M/s | 35.34 M/s | — | — |
| `packstore.has_outside` | sealed-segment | — | — | — | 21.74 M/s | 26.87 M/s | — | — |
| `packstore.liveness` | tenth-live | — | — | — | 31.15 k/s | 32.72 k/s | — | — |
| `packstore.mark_set_contains` | all-keys | — | — | — | 10.79 M/s | 11.59 M/s | — | — |
| `packstore.mark_set_mark` | all-keys | — | — | — | 16.40 M/s | 21.84 M/s | — | — |
| `packstore.missing` | half-present | — | — | — | 9.49 M/s | 1.99 M/s | — | — |
| `packstore.new_mark_set` | snapshot | — | — | — | 51.96 k/s | 80.98 k/s | — | — |
| `packstore.oldest_inflight_write` | idle | — | — | — | 71.50 M/s | 311.60 M/s | — | — |
| `packstore.open` | empty | — | — | — | 43.25 k/s | 72.53 k/s | — | — |
| `packstore.open` | populated-reopen | — | — | — | 4.77 k/s | 3.95 k/s | — | — |
| `packstore.put` | 1MiB-duplicate/sync-off | payload bytes | 1381.9 MiB/s | 3630.9 MiB/s | 1.38 k/s | 3.63 k/s | — | — |
| `packstore.put` | 1MiB-random/sync-off | payload bytes | 926.3 MiB/s | 1945.7 MiB/s | 926 /s | 1.95 k/s | — | — |
| `packstore.put` | 1MiB-text/sync-off | payload bytes | 249.6 MiB/s | 509.3 MiB/s | 250 /s | 509 /s | — | — |
| `packstore.put` | 4KiB-duplicate/sync-off | payload bytes | 714.4 MiB/s | 984.5 MiB/s | 182.89 k/s | 252.02 k/s | — | — |
| `packstore.put` | 4KiB-random/already-present | payload bytes | 28545.3 MiB/s | 45871.6 MiB/s | 7.31 M/s | 11.74 M/s | — | — |
| `packstore.put` | 4KiB-random/sync-off | payload bytes | 223.5 MiB/s | 373.0 MiB/s | 57.20 k/s | 95.48 k/s | — | — |
| `packstore.put` | 4KiB-random/sync-on | payload bytes | 40.3 MiB/s | 42.6 MiB/s | 10.32 k/s | 10.90 k/s | — | — |
| `packstore.put` | 4KiB-text/sync-off | payload bytes | 71.2 MiB/s | 200.4 MiB/s | 18.22 k/s | 51.30 k/s | — | — |
| `packstore.put` | empty/sync-off | payload bytes | — | — | 3.80 M/s | 4.64 M/s | — | — |
| `packstore.put` | large-8MiB-duplicate/sync-off | payload bytes | 930.4 MiB/s | 3071.6 MiB/s | 116 /s | 384 /s | — | — |
| `packstore.put` | large-8MiB-random/sync-off | payload bytes | 912.4 MiB/s | 1148.7 MiB/s | 114 /s | 144 /s | — | — |
| `packstore.put` | large-8MiB-text/sync-off | payload bytes | 296.7 MiB/s | 540.7 MiB/s | 37 /s | 68 /s | — | — |
| `packstore.put` | tiny-64B-duplicate/sync-off | payload bytes | 376.8 MiB/s | 406.7 MiB/s | 6.17 M/s | 6.66 M/s | — | — |
| `packstore.put` | tiny-64B-random/sync-off | payload bytes | 26.4 MiB/s | 34.6 MiB/s | 432.04 k/s | 567.34 k/s | — | — |
| `packstore.put` | tiny-64B-text/sync-off | payload bytes | 24.0 MiB/s | 26.1 MiB/s | 393.65 k/s | 427.32 k/s | — | — |
| `packstore.put_verified` | already-intact | — | — | — | 12.30 k/s | 19.12 k/s | — | — |
| `packstore.record` | by-location | — | — | — | 1.20 M/s | 1.21 M/s | — | — |
| `packstore.remove` | one-sealed-segment | — | — | — | 9.48 k/s | 10.32 k/s | — | — |
| `packstore.scan_index` | all-segments | — | — | — | 33.10 M/s | 43.07 M/s | — | — |
| `packstore.segments` | list | — | — | — | 358.36 k/s | 478.59 k/s | — | — |
| `packstore.sort_by_location` | scattered | — | — | — | 5.53 M/s | 9.71 M/s | — | — |
| `packstore.stored_size` | hit | — | — | — | 13.80 M/s | 17.95 M/s | — | — |
| `packstore.verify` | full-scrub | — | — | — | 128.95 k/s | 207.10 k/s | — | — |
| `packstore.wipe` | populated | — | — | — | 4.08 k/s | 4.44 k/s | — | — |
| `packstore.write_batch` | mixed-objects | payload bytes | 245.5 MiB/s | 375.8 MiB/s | 29.14 k/s | 44.61 k/s | — | — |
| `packstore.write_parallel` | duplicate-stream/writers-N | payload bytes | 30842.9 MiB/s | 9810.3 MiB/s | 3.66 M/s | 1.16 M/s | — | — |
| `packstore.write_parallel` | mixed-objects/writers-1/verify-false | payload bytes | 247.4 MiB/s | 372.5 MiB/s | 29.37 k/s | 44.22 k/s | — | — |
| `packstore.write_parallel` | mixed-objects/writers-1/verify-true | payload bytes | 236.6 MiB/s | 349.6 MiB/s | 28.09 k/s | 41.50 k/s | — | — |
| `packstore.write_parallel` | mixed-objects/writers-N/verify-false | payload bytes | 721.4 MiB/s | 1170.3 MiB/s | 85.65 k/s | 138.94 k/s | — | — |
| `packstore.write_parallel` | mixed-objects/writers-N/verify-true | payload bytes | 673.7 MiB/s | 1038.6 MiB/s | 79.98 k/s | 123.30 k/s | — | — |
| `reference.decode` | signed | encoded bytes | 350.5 MiB/s | 684.3 MiB/s | 1.16 M/s | 2.26 M/s | — | — |
| `reference.encode` | signed | encoded bytes | 1192.2 MiB/s | 1760.0 MiB/s | 3.94 M/s | 5.82 M/s | — | — |
| `reference.signature_payload` | signed | — | — | — | 4.00 M/s | 6.00 M/s | — | — |
| `reference.validate_name` | valid | — | — | — | 15.03 M/s | 30.02 M/s | — | — |
| `reference.validate_user` | valid | — | — | — | 13.98 M/s | 10.34 M/s | — | — |
| `refstore.all` | records | — | — | — | 2.03 M/s | 5.28 M/s | — | — |
| `refstore.close` | populated | — | — | — | 6.95 k/s | 1.04 k/s | — | — |
| `refstore.delete` | records | — | — | — | 599.13 k/s | 27.43 k/s | — | — |
| `refstore.get` | hit | — | — | — | 906.08 k/s | 2.29 M/s | — | — |
| `refstore.get` | miss | — | — | — | 4.53 M/s | 3.10 M/s | — | — |
| `refstore.open` | empty | — | — | — | 1.38 k/s | 806 /s | — | — |
| `refstore.open` | populated-reopen | — | — | — | 961 /s | 1.32 k/s | — | — |
| `refstore.put` | records/sync-false | — | — | — | 2.62 M/s | 24.88 k/s | — | — |
| `refstore.put` | records/sync-true | — | — | — | 16.44 k/s | 25.28 k/s | — | — |
| `refstore.put_batch` | records/sync-false | — | — | — | 9.94 M/s | 840.46 k/s | — | — |
| `refstore.wipe` | records | — | — | — | 7.30 k/s | 2.82 k/s | — | — |
| `tarexport.write` | fixture-tree | encoded bytes | 8620.1 MiB/s | 6500.9 MiB/s | 385.63 k/s | 290.83 k/s | — | — |
| `tarextract.extract` | fixture-tree | encoded bytes | 545.6 MiB/s | 607.1 MiB/s | 24.41 k/s | 27.16 k/s | — | — |

## Scaling

Each row is one dimension swept with everything else about the case held
fixed, so the trend is a statement about that dimension. `per unit` is the
time divided by the swept dimension at its largest point: where it is flat
across the row the cost is proportional, where it falls the fixed overhead
dominated the small end.

### `amberpack.encode_record` over object size (bytes) (duplicate)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-duplicate | 503.3 ns | 597.3 ns | 0.84× | 7.9 ns | 9.3 ns |
| 4,096 | 4KiB-duplicate | 4.002 us | 4.134 us | 0.97× | 1.0 ns | 1.0 ns |
| 1,048,576 | 1MiB-duplicate | 603.874 us | 285.536 us | 2.11× | 0.6 ns | 0.3 ns |
| 8,388,608 | large-8MiB-duplicate | 14.042 ms | 5.106 ms | 2.75× | 1.7 ns | 0.6 ns |

### `amberpack.encode_record` over object size (bytes) (random)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-random | 562.9 ns | 628.5 ns | 0.90× | 8.8 ns | 9.8 ns |
| 4,096 | 4KiB-random | 9.786 us | 4.130 us | 2.37× | 2.4 ns | 1.0 ns |
| 1,048,576 | 1MiB-random | 517.950 us | 278.252 us | 1.86× | 0.5 ns | 0.3 ns |
| 8,388,608 | large-8MiB-random | 6.098 ms | 6.229 ms | 0.98× | 0.7 ns | 0.7 ns |

### `amberpack.encode_record` over object size (bytes) (text)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-text | 1.152 us | 1.172 us | 0.98× | 18.0 ns | 18.3 ns |
| 4,096 | 4KiB-text | 27.117 us | 13.649 us | 1.99× | 6.6 ns | 3.3 ns |
| 1,048,576 | 1MiB-text | 3.911 ms | 1.804 ms | 2.17× | 3.7 ns | 1.7 ns |
| 8,388,608 | large-8MiB-text | 26.298 ms | 14.160 ms | 1.86× | 3.1 ns | 1.7 ns |

### `fstree.check_complete` over directory width (plain)

| directory width | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per width | Rust ns per width |
|---:|---|---:|---:|---:|---:|---:|
| 16 | width-16/jobs-1 | 1.899 us | 580.5 ns | 3.27× | 2.018 us | 616.8 ns |
| 256 | width-256/jobs-1 | 762.5 ns | 361.8 ns | 2.11× | 771.4 ns | 366.1 ns |
| 4,096 | width-4096/jobs-1 | 662.9 ns | 314.8 ns | 2.11× | 667.1 ns | 316.8 ns |
| 40,000 | width-40000/jobs-1 | 741.0 ns | 272.2 ns | 2.72× | 745.7 ns | 274.0 ns |

### `fstree.collect_entries` over directory width (plain)

| directory width | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per width | Rust ns per width |
|---:|---|---:|---:|---:|---:|---:|
| 16 | width-16 | 679.4 ns | 735.1 ns | 0.92× | 679.4 ns | 735.1 ns |
| 256 | width-256 | 392.1 ns | 216.0 ns | 1.81× | 392.1 ns | 216.0 ns |
| 4,096 | width-4096 | 367.4 ns | 215.4 ns | 1.71× | 367.4 ns | 215.4 ns |
| 40,000 | width-40000 | 633.2 ns | 217.3 ns | 2.91× | 633.2 ns | 217.3 ns |

### `fstree.decode_dir_leaf` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 8 | entries-8 | 375.6 ns | 216.3 ns | 1.74× | 375.6 ns | 216.3 ns |
| 128 | entries-128 | 361.4 ns | 212.4 ns | 1.70× | 361.4 ns | 212.4 ns |
| 1,024 | entries-1024 | 365.8 ns | 213.4 ns | 1.71× | 365.8 ns | 213.4 ns |

### `fstree.decode_dir_node` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 8 | pairs-8 | 189.2 ns | 87.7 ns | 2.16× | 189.2 ns | 87.7 ns |
| 128 | pairs-128 | 168.0 ns | 98.5 ns | 1.71× | 168.0 ns | 98.5 ns |
| 1,024 | pairs-1024 | 175.4 ns | 96.9 ns | 1.81× | 175.4 ns | 96.9 ns |

### `fstree.decode_file_node` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 8 | children-8 | 110.6 ns | 67.1 ns | 1.65× | 110.6 ns | 67.1 ns |
| 128 | children-128 | 88.7 ns | 67.8 ns | 1.31× | 88.7 ns | 67.8 ns |
| 1,024 | children-1024 | 86.2 ns | 69.4 ns | 1.24× | 86.2 ns | 69.4 ns |
| 65,536 | children-65536 | 99.9 ns | 70.1 ns | 1.43× | 99.9 ns | 70.1 ns |

### `fstree.dir_builder` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 16 | entries-16 | 2.264 us | 557.9 ns | 4.06× | 2.264 us | 557.9 ns |
| 256 | entries-256 | 592.3 ns | 346.3 ns | 1.71× | 592.3 ns | 346.3 ns |
| 4,096 | entries-4096 | 485.1 ns | 308.5 ns | 1.57× | 485.1 ns | 308.5 ns |
| 40,000 | entries-40000 | 507.2 ns | 320.6 ns | 1.58× | 507.2 ns | 320.6 ns |

### `fstree.encode_blob` over object size (bytes) (duplicate)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-duplicate | 136.0 ns | 110.6 ns | 1.23× | 2.1 ns | 1.7 ns |
| 4,096 | 4KiB-duplicate | 1.827 us | 1.137 us | 1.61× | 0.4 ns | 0.3 ns |
| 1,048,576 | 1MiB-duplicate | 218.302 us | 137.688 us | 1.59× | 0.2 ns | 0.1 ns |
| 8,388,608 | large-8MiB-duplicate | 1.755 ms | 1.150 ms | 1.53× | 0.2 ns | 0.1 ns |

### `fstree.encode_blob` over object size (bytes) (random)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-random | 135.7 ns | 110.4 ns | 1.23× | 2.1 ns | 1.7 ns |
| 4,096 | 4KiB-random | 1.959 us | 1.137 us | 1.72× | 0.5 ns | 0.3 ns |
| 1,048,576 | 1MiB-random | 246.140 us | 137.152 us | 1.79× | 0.2 ns | 0.1 ns |
| 8,388,608 | large-8MiB-random | 1.923 ms | 1.091 ms | 1.76× | 0.2 ns | 0.1 ns |

### `fstree.encode_blob` over object size (bytes) (text)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-text | 137.2 ns | 111.4 ns | 1.23× | 2.1 ns | 1.7 ns |
| 4,096 | 4KiB-text | 2.130 us | 1.150 us | 1.85× | 0.5 ns | 0.3 ns |
| 1,048,576 | 1MiB-text | 262.411 us | 152.752 us | 1.72× | 0.3 ns | 0.1 ns |
| 8,388,608 | large-8MiB-text | 1.930 ms | 1.092 ms | 1.77× | 0.2 ns | 0.1 ns |

### `fstree.encode_dir_leaf` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 8 | entries-8 | 248.9 ns | 132.2 ns | 1.88× | 248.9 ns | 132.2 ns |
| 128 | entries-128 | 164.9 ns | 58.9 ns | 2.80× | 164.9 ns | 58.9 ns |
| 1,024 | entries-1024 | 159.5 ns | 46.7 ns | 3.42× | 159.5 ns | 46.7 ns |

### `fstree.encode_dir_node` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 8 | pairs-8 | 136.7 ns | 90.1 ns | 1.52× | 136.7 ns | 90.1 ns |
| 128 | pairs-128 | 71.3 ns | 43.0 ns | 1.66× | 71.3 ns | 43.0 ns |
| 1,024 | pairs-1024 | 67.6 ns | 30.9 ns | 2.19× | 67.6 ns | 30.9 ns |

### `fstree.encode_file_node` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 8 | children-8 | 117.6 ns | 47.0 ns | 2.50× | 117.6 ns | 47.0 ns |
| 128 | children-128 | 59.0 ns | 19.2 ns | 3.08× | 59.0 ns | 19.2 ns |
| 1,024 | children-1024 | 39.0 ns | 13.3 ns | 2.94× | 39.0 ns | 13.3 ns |
| 65,536 | children-65536 | 54.6 ns | 12.5 ns | 4.38× | 54.6 ns | 12.5 ns |

### `fstree.index_builder_file` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 8 | children-8 | 1.325 us | 97.8 ns | 13.56× | 1.325 us | 97.8 ns |
| 128 | children-128 | 220.9 ns | 70.6 ns | 3.13× | 220.9 ns | 70.6 ns |
| 1,024 | children-1024 | 136.6 ns | 70.9 ns | 1.93× | 136.6 ns | 70.9 ns |
| 65,536 | children-65536 | 128.4 ns | 69.3 ns | 1.85× | 128.4 ns | 69.3 ns |

### `fstree.list_entries` over directory width (plain)

| directory width | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per width | Rust ns per width |
|---:|---|---:|---:|---:|---:|---:|
| 16 | width-16/full-paging-100 | 617.4 ns | 638.8 ns | 0.97× | 617.4 ns | 638.8 ns |
| 256 | width-256/full-paging-100 | 692.4 ns | 402.8 ns | 1.72× | 692.4 ns | 402.8 ns |
| 4,096 | width-4096/full-paging-100 | 1.151 us | 712.3 ns | 1.62× | 1.151 us | 712.3 ns |
| 40,000 | width-40000/full-paging-100 | 1.357 us | 832.1 ns | 1.63× | 1.357 us | 832.1 ns |

### `fstree.lookup_entry` over directory width (hit)

| directory width | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per width | Rust ns per width |
|---:|---|---:|---:|---:|---:|---:|
| 16 | width-16/hit | 5.150 us | 2.909 us | 1.77× | 64.381 us | 36.357 us |
| 256 | width-256/hit | 40.846 us | 25.567 us | 1.60× | 31.911 us | 19.974 us |
| 4,096 | width-4096/hit | 88.838 us | 52.784 us | 1.68× | 4.338 us | 2.577 us |
| 40,000 | width-40000/hit | 103.642 us | 64.280 us | 1.61× | 518.2 ns | 321.4 ns |

### `fstree.lookup_entry` over directory width (miss)

| directory width | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per width | Rust ns per width |
|---:|---|---:|---:|---:|---:|---:|
| 16 | width-16/miss | 5.567 us | 2.909 us | 1.91× | 69.587 us | 36.363 us |
| 256 | width-256/miss | 711.4 ns | 220.6 ns | 3.23× | 555.8 ns | 172.3 ns |
| 4,096 | width-4096/miss | 3.736 us | 2.028 us | 1.84× | 182.4 ns | 99.0 ns |
| 40,000 | width-40000/miss | 736.1 ns | 220.3 ns | 3.34× | 3.7 ns | 1.1 ns |

### `fstree.reachable_keys` over directory width (plain)

| directory width | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per width | Rust ns per width |
|---:|---|---:|---:|---:|---:|---:|
| 16 | width-16 | 1.476 us | 6.314 us | 0.23× | 1.568 us | 6.709 us |
| 256 | width-256 | 485.5 ns | 700.2 ns | 0.69× | 491.2 ns | 708.4 ns |
| 4,096 | width-4096 | 142.5 ns | 143.5 ns | 0.99× | 143.4 ns | 144.4 ns |
| 40,000 | width-40000 | 227.6 ns | 108.8 ns | 2.09× | 229.0 ns | 109.5 ns |

### `fstree.resolve_entry` over path depth (plain)

| path depth | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per depth | Rust ns per depth |
|---:|---|---:|---:|---:|---:|---:|
| 1 | depth-1 | 6.068 us | 3.390 us | 1.79× | 6.068 us | 3.390 us |
| 4 | depth-4 | 8.793 us | 4.663 us | 1.89× | 2.198 us | 1.166 us |
| 12 | depth-12 | 15.842 us | 8.208 us | 1.93× | 1.320 us | 684.0 ns |
| 24 | depth-24 | 26.880 us | 13.564 us | 1.98× | 1.120 us | 565.2 ns |

### `fstree.resolve_path` over path depth (plain)

| path depth | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per depth | Rust ns per depth |
|---:|---|---:|---:|---:|---:|---:|
| 1 | depth-1 | 907.4 ns | 463.8 ns | 1.96× | 907.4 ns | 463.8 ns |
| 4 | depth-4 | 3.445 us | 1.818 us | 1.89× | 861.4 ns | 454.6 ns |
| 12 | depth-12 | 10.383 us | 5.249 us | 1.98× | 865.2 ns | 437.4 ns |
| 24 | depth-24 | 20.649 us | 10.567 us | 1.95× | 860.4 ns | 440.3 ns |

### `key.new` over object size (bytes) (duplicate)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-duplicate | 108.2 ns | 84.0 ns | 1.29× | 1.7 ns | 1.3 ns |
| 4,096 | 4KiB-duplicate | 1.781 us | 1.329 us | 1.34× | 0.4 ns | 0.3 ns |
| 1,048,576 | 1MiB-duplicate | 219.595 us | 175.290 us | 1.25× | 0.2 ns | 0.2 ns |
| 8,388,608 | large-8MiB-duplicate | 1.927 ms | 1.404 ms | 1.37× | 0.2 ns | 0.2 ns |

### `key.new` over object size (bytes) (random)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-random | 106.8 ns | 86.5 ns | 1.23× | 1.7 ns | 1.4 ns |
| 4,096 | 4KiB-random | 1.890 us | 1.330 us | 1.42× | 0.5 ns | 0.3 ns |
| 1,048,576 | 1MiB-random | 257.923 us | 171.778 us | 1.50× | 0.2 ns | 0.2 ns |
| 8,388,608 | large-8MiB-random | 1.921 ms | 1.338 ms | 1.44× | 0.2 ns | 0.2 ns |

### `key.new` over object size (bytes) (text)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-text | 109.0 ns | 86.5 ns | 1.26× | 1.7 ns | 1.4 ns |
| 4,096 | 4KiB-text | 2.087 us | 1.319 us | 1.58× | 0.5 ns | 0.3 ns |
| 1,048,576 | 1MiB-text | 249.617 us | 172.810 us | 1.44× | 0.2 ns | 0.2 ns |
| 8,388,608 | large-8MiB-text | 1.919 ms | 1.481 ms | 1.30× | 0.2 ns | 0.2 ns |

### `packstore.put` over object size (bytes) (duplicate)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-duplicate/sync-off | 162.0 ns | 150.1 ns | 1.08× | 2.5 ns | 2.3 ns |
| 4,096 | 4KiB-duplicate/sync-off | 5.468 us | 3.968 us | 1.38× | 1.3 ns | 1.0 ns |
| 1,048,576 | 1MiB-duplicate/sync-off | 723.622 us | 275.411 us | 2.63× | 0.7 ns | 0.3 ns |
| 8,388,608 | large-8MiB-duplicate/sync-off | 8.598 ms | 2.604 ms | 3.30× | 1.0 ns | 0.3 ns |

### `packstore.put` over object size (bytes) (text)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-text/sync-off | 2.540 us | 2.340 us | 1.09× | 39.7 ns | 36.6 ns |
| 4,096 | 4KiB-text/sync-off | 54.895 us | 19.493 us | 2.82× | 13.4 ns | 4.8 ns |
| 1,048,576 | 1MiB-text/sync-off | 4.007 ms | 1.964 ms | 2.04× | 3.8 ns | 1.9 ns |
| 8,388,608 | large-8MiB-text/sync-off | 26.966 ms | 14.796 ms | 1.82× | 3.2 ns | 1.8 ns |

## Worker scaling

The same operation asked for one worker and for the profile's concurrent
count. Speed-up is the single-worker time divided by the concurrent time;
both cores were pinned to the same CPU set.

| Operation | Workload | Workers | Go 1 | Go n | Go speed-up | Rust 1 | Rust n | Rust speed-up |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| `fstree.check_complete` | widest/jobs-* | 4 | 794.2 ns | 448.2 ns | 1.77× | 355.4 ns | 168.0 ns | 2.12× |
| `ingest.dir` | tree/fresh-store/jobs-* | 4 | 222.121 us | 103.986 us | 2.14× | 43.492 us | 29.694 us | 1.46× |
| `ingest.dir` | tree/incremental-change/jobs-* | 4 | 64.412 us | 76.084 us | 0.85× | 20.927 us | 19.856 us | 1.05× |
| `ingest.dir` | tree/unchanged-repeat/jobs-* | 4 | 82.117 us | 61.240 us | 1.34× | 24.786 us | 18.800 us | 1.32× |
| `ingest.objects` | tree/jobs-* | 4 | 75.703 us | 56.080 us | 1.35× | 22.607 us | 19.279 us | 1.17× |
| `ingest.scan` | filtered/jobs-* | 4 | 2.693 us | 1.410 us | 1.91× | 2.927 us | 3.132 us | 0.93× |
| `packstore.write_parallel` | mixed-objects/writers-*/verify-false | 4 | 34.047 us | 11.675 us | 2.92× | 22.613 us | 7.197 us | 3.14× |
| `packstore.write_parallel` | mixed-objects/writers-*/verify-true | 4 | 35.594 us | 12.503 us | 2.85× | 24.095 us | 8.110 us | 2.97× |

## CPU time

Process CPU time (user + system) attributed to the measured interval, per
operation. It is the one resource figure the two runtimes report the same
way; for concurrent cases it exceeds the elapsed time.

| Operation | Workload | Thr | Go CPU ns/op | Rust CPU ns/op | Go/Rust |
|---|---|---|---:|---:|---:|
| `amberignore.descend` | 8-subdirs | 1 | 5.256 us | 3.273 us | 1.61× |
| `amberignore.ignored` | mixed-names | 1 | 210.0 ns | 112.3 ns | 1.87× |
| `amberignore.ignored` | nil-matcher | 1 | 7.8 ns | 1.0 ns | 8.00× |
| `amberignore.root` | load-root | 1 | 7.562 us | 4.406 us | 1.72× |
| `amberpack.decode_payload` | mixed-records/producer-go | 1 | 16.172 us | 10.312 us | 1.57× |
| `amberpack.decode_payload` | mixed-records/producer-rust | 1 | 14.594 us | 9.438 us | 1.55× |
| `amberpack.encode_record` | 1MiB-duplicate | 1 | 602.000 us | 286.500 us | 2.10× |
| `amberpack.encode_record` | 1MiB-random | 1 | 518.000 us | 265.000 us | 1.95× |
| `amberpack.encode_record` | 1MiB-text | 1 | 3.903 ms | 1.789 ms | 2.18× |
| `amberpack.encode_record` | 4KiB-duplicate | 1 | 3.996 us | 4.129 us | 0.97× |
| `amberpack.encode_record` | 4KiB-random | 1 | 13.594 us | 4.125 us | 3.30× |
| `amberpack.encode_record` | 4KiB-text | 1 | 28.176 us | 13.621 us | 2.07× |
| `amberpack.encode_record` | empty | 1 | 76.2 ns | 281.2 ns | 0.27× |
| `amberpack.encode_record` | large-8MiB-duplicate | 1 | 14.470 ms | 5.081 ms | 2.85× |
| `amberpack.encode_record` | large-8MiB-random | 1 | 7.096 ms | 6.193 ms | 1.15× |
| `amberpack.encode_record` | large-8MiB-text | 1 | 27.180 ms | 14.090 ms | 1.93× |
| `amberpack.encode_record` | tiny-64B-duplicate | 1 | 566.8 ns | 595.2 ns | 0.95× |
| `amberpack.encode_record` | tiny-64B-random | 1 | 575.8 ns | 626.3 ns | 0.92× |
| `amberpack.encode_record` | tiny-64B-text | 1 | 1.149 us | 1.168 us | 0.98× |
| `amberpack.parse_record` | corrupt-crc/producer-go | 1 | 187.5 ns | 105.5 ns | 1.78× |
| `amberpack.parse_record` | corrupt-crc/producer-rust | 1 | 171.9 ns | 101.6 ns | 1.69× |
| `amberpack.parse_record` | mixed-records/producer-go | 1 | 2.266 us | 2.547 us | 0.89× |
| `amberpack.parse_record` | mixed-records/producer-rust | 1 | 2.234 us | 2.594 us | 0.86× |
| `amberpack.reader_all` | mixed-objects/producer-go | 1 | 17.922 us | 15.172 us | 1.18× |
| `amberpack.reader_all` | mixed-objects/producer-rust | 1 | 17.000 us | 13.656 us | 1.24× |
| `amberpack.reader_all` | truncated-stream/producer-go | 1 | 965.078 us | 424.672 us | 2.27× |
| `amberpack.reader_all` | truncated-stream/producer-rust | 1 | 549.312 us | 400.797 us | 1.37× |
| `amberpack.reader_records` | mixed-objects/producer-go | 1 | 2.609 us | 2.859 us | 0.91× |
| `amberpack.reader_records` | mixed-objects/producer-rust | 1 | 1.844 us | 2.844 us | 0.65× |
| `amberpack.writer_add` | mixed-objects | 1 | 107.688 us | 40.797 us | 2.64× |
| `amberpack.writer_add_record` | pre-encoded-records/producer-go | 1 | 203.1 ns | 15.6 ns | 13.00× |
| `amberpack.writer_add_record` | pre-encoded-records/producer-rust | 1 | 218.8 ns | 15.6 ns | 14.00× |
| `cbor.decode_xattrs` | xattrs-3 | 1 | 468.8 ns | 203.1 ns | 2.31× |
| `cbor.decode_xattrs` | xattrs-64 | 1 | 9.484 us | 5.609 us | 1.69× |
| `cbor.encode_xattrs` | xattrs-3 | 1 | 531.2 ns | 281.2 ns | 1.89× |
| `cbor.encode_xattrs` | xattrs-64 | 1 | 26.734 us | 4.688 us | 5.70× |
| `chunkers.item_chunker` | is_boundary/bits-7 | 1 | 95.0 ns | 35.0 ns | 2.71× |
| `chunkers.new_item_chunker` | bits-4..12 | 1 | 15.6 ns | 13.0 ns | 1.20× |
| `chunkers.split_bytes` | compressible/4-16-64KiB | 1 | 54.172 us | 42.219 us | 1.28× |
| `chunkers.split_bytes` | compressible/default-sizes | 1 | 1.333 ms | 662.000 us | 2.01× |
| `chunkers.split_bytes` | random/default-sizes | 1 | 51.370 us | 46.037 us | 1.12× |
| `chunkers.split_bytes` | tiny-64KiB/default-sizes | 1 | 54.000 us | 23.000 us | 2.35× |
| `fstree.check_complete` | incomplete/jobs-1 | 1 | 71.000 us | 15.000 us | 4.73× |
| `fstree.check_complete` | widest/jobs-1 | 1 | 826.7 ns | 354.1 ns | 2.33× |
| `fstree.check_complete` | widest/jobs-N | 4 | 842.5 ns | 509.7 ns | 1.65× |
| `fstree.check_complete` | width-16/jobs-1 | 1 | 2.588 us | 588.2 ns | 4.40× |
| `fstree.check_complete` | width-256/jobs-1 | 1 | 826.3 ns | 355.2 ns | 2.33× |
| `fstree.check_complete` | width-40000/jobs-1 | 1 | 769.9 ns | 271.2 ns | 2.84× |
| `fstree.check_complete` | width-4096/jobs-1 | 1 | 692.9 ns | 314.2 ns | 2.21× |
| `fstree.child_keys` | dir-leaf-128 | 1 | 47.375 us | 29.422 us | 1.61× |
| `fstree.child_keys` | dir-node-128 | 1 | 23.906 us | 14.625 us | 1.63× |
| `fstree.child_keys` | file-node-1024 | 1 | 85.953 us | 72.297 us | 1.19× |
| `fstree.collect_entries` | width-16 | 1 | 875.0 ns | 750.0 ns | 1.17× |
| `fstree.collect_entries` | width-256 | 1 | 402.3 ns | 218.8 ns | 1.84× |
| `fstree.collect_entries` | width-40000 | 1 | 631.3 ns | 216.8 ns | 2.91× |
| `fstree.collect_entries` | width-4096 | 1 | 367.7 ns | 214.8 ns | 1.71× |
| `fstree.decode_dir_leaf` | entries-1024 | 1 | 378.4 ns | 212.7 ns | 1.78× |
| `fstree.decode_dir_leaf` | entries-128 | 1 | 361.0 ns | 212.0 ns | 1.70× |
| `fstree.decode_dir_leaf` | entries-128-partial-change | 1 | 444.7 ns | 222.8 ns | 2.00× |
| `fstree.decode_dir_leaf` | entries-128-with-xattrs | 1 | 368.8 ns | 221.7 ns | 1.66× |
| `fstree.decode_dir_leaf` | entries-8 | 1 | 380.9 ns | 212.9 ns | 1.79× |
| `fstree.decode_dir_node` | pairs-1024 | 1 | 182.3 ns | 96.5 ns | 1.89× |
| `fstree.decode_dir_node` | pairs-128 | 1 | 168.7 ns | 98.4 ns | 1.71× |
| `fstree.decode_dir_node` | pairs-8 | 1 | 195.3 ns | 89.8 ns | 2.17× |
| `fstree.decode_file_node` | children-1024 | 1 | 86.0 ns | 69.1 ns | 1.24× |
| `fstree.decode_file_node` | children-128 | 1 | 89.6 ns | 67.6 ns | 1.32× |
| `fstree.decode_file_node` | children-65536 | 1 | 101.3 ns | 69.8 ns | 1.45× |
| `fstree.decode_file_node` | children-8 | 1 | 117.2 ns | 68.4 ns | 1.71× |
| `fstree.dir_builder` | entries-16 | 1 | 2.625 us | 625.0 ns | 4.20× |
| `fstree.dir_builder` | entries-256 | 1 | 589.8 ns | 351.6 ns | 1.68× |
| `fstree.dir_builder` | entries-40000 | 1 | 530.7 ns | 319.4 ns | 1.66× |
| `fstree.dir_builder` | entries-4096 | 1 | 484.1 ns | 308.1 ns | 1.57× |
| `fstree.encode_blob` | 1MiB-duplicate | 1 | 219.000 us | 138.500 us | 1.58× |
| `fstree.encode_blob` | 1MiB-random | 1 | 249.000 us | 138.000 us | 1.80× |
| `fstree.encode_blob` | 1MiB-text | 1 | 348.000 us | 138.000 us | 2.52× |
| `fstree.encode_blob` | 4KiB-duplicate | 1 | 1.844 us | 1.137 us | 1.62× |
| `fstree.encode_blob` | 4KiB-random | 1 | 1.973 us | 1.141 us | 1.73× |
| `fstree.encode_blob` | 4KiB-text | 1 | 2.184 us | 1.145 us | 1.91× |
| `fstree.encode_blob` | empty | 1 | 135.0 ns | 91.2 ns | 1.48× |
| `fstree.encode_blob` | large-8MiB-duplicate | 1 | 2.231 ms | 1.145 ms | 1.95× |
| `fstree.encode_blob` | large-8MiB-random | 1 | 1.921 ms | 1.089 ms | 1.76× |
| `fstree.encode_blob` | large-8MiB-text | 1 | 1.926 ms | 1.090 ms | 1.77× |
| `fstree.encode_blob` | tiny-64B-duplicate | 1 | 134.6 ns | 110.5 ns | 1.22× |
| `fstree.encode_blob` | tiny-64B-random | 1 | 135.7 ns | 110.1 ns | 1.23× |
| `fstree.encode_blob` | tiny-64B-text | 1 | 136.8 ns | 111.1 ns | 1.23× |
| `fstree.encode_dir_leaf` | entries-1024 | 1 | 159.0 ns | 46.6 ns | 3.41× |
| `fstree.encode_dir_leaf` | entries-128 | 1 | 164.7 ns | 58.7 ns | 2.80× |
| `fstree.encode_dir_leaf` | entries-128-partial-change | 1 | 177.5 ns | 66.5 ns | 2.67× |
| `fstree.encode_dir_leaf` | entries-128-with-xattrs | 1 | 174.7 ns | 66.9 ns | 2.61× |
| `fstree.encode_dir_leaf` | entries-8 | 1 | 265.6 ns | 132.8 ns | 2.00× |
| `fstree.encode_dir_node` | pairs-1024 | 1 | 67.9 ns | 30.8 ns | 2.20× |
| `fstree.encode_dir_node` | pairs-128 | 1 | 72.0 ns | 43.0 ns | 1.68× |
| `fstree.encode_dir_node` | pairs-8 | 1 | 140.6 ns | 89.8 ns | 1.57× |
| `fstree.encode_file_node` | children-1024 | 1 | 38.8 ns | 13.2 ns | 2.93× |
| `fstree.encode_file_node` | children-128 | 1 | 59.7 ns | 19.3 ns | 3.09× |
| `fstree.encode_file_node` | children-65536 | 1 | 57.8 ns | 12.4 ns | 4.66× |
| `fstree.encode_file_node` | children-8 | 1 | 121.1 ns | 46.9 ns | 2.58× |
| `fstree.encode_xattr_set` | xattrs-64 | 1 | 487.3 ns | 99.4 ns | 4.90× |
| `fstree.index_builder_file` | children-1024 | 1 | 139.6 ns | 71.3 ns | 1.96× |
| `fstree.index_builder_file` | children-128 | 1 | 265.6 ns | 78.1 ns | 3.40× |
| `fstree.index_builder_file` | children-65536 | 1 | 127.8 ns | 69.2 ns | 1.85× |
| `fstree.index_builder_file` | children-8 | 1 | 1.750 us | 125.0 ns | 14.00× |
| `fstree.list_entries` | widest/first-page-100 | 1 | 1.530 us | 852.5 ns | 1.79× |
| `fstree.list_entries` | width-16/full-paging-100 | 1 | 750.0 ns | 625.0 ns | 1.20× |
| `fstree.list_entries` | width-256/full-paging-100 | 1 | 707.0 ns | 402.3 ns | 1.76× |
| `fstree.list_entries` | width-40000/full-paging-100 | 1 | 1.384 us | 829.0 ns | 1.67× |
| `fstree.list_entries` | width-4096/full-paging-100 | 1 | 1.146 us | 710.9 ns | 1.61× |
| `fstree.lookup_entry` | width-16/hit | 1 | 5.140 us | 2.900 us | 1.77× |
| `fstree.lookup_entry` | width-16/miss | 1 | 5.510 us | 2.910 us | 1.89× |
| `fstree.lookup_entry` | width-256/hit | 1 | 40.700 us | 25.455 us | 1.60× |
| `fstree.lookup_entry` | width-256/miss | 1 | 730.0 ns | 225.0 ns | 3.24× |
| `fstree.lookup_entry` | width-40000/hit | 1 | 108.015 us | 64.030 us | 1.69× |
| `fstree.lookup_entry` | width-40000/miss | 1 | 740.0 ns | 225.0 ns | 3.29× |
| `fstree.lookup_entry` | width-4096/hit | 1 | 88.885 us | 52.585 us | 1.69× |
| `fstree.lookup_entry` | width-4096/miss | 1 | 3.740 us | 2.030 us | 1.84× |
| `fstree.reachable_keys` | deep | auto | 7.375 us | 82.000 us | 0.09× |
| `fstree.reachable_keys` | file-corpus | auto | 5.000 us | 30.500 us | 0.16× |
| `fstree.reachable_keys` | width-16 | auto | 2.118 us | 12.235 us | 0.17× |
| `fstree.reachable_keys` | width-256 | auto | 563.7 ns | 1.286 us | 0.44× |
| `fstree.reachable_keys` | width-40000 | auto | 519.5 ns | 415.1 ns | 1.25× |
| `fstree.reachable_keys` | width-4096 | auto | 428.4 ns | 448.3 ns | 0.96× |
| `fstree.resolve_entry` | depth-1 | 1 | 6.074 us | 4.105 us | 1.48× |
| `fstree.resolve_entry` | depth-12 | 1 | 15.809 us | 8.188 us | 1.93× |
| `fstree.resolve_entry` | depth-24 | 1 | 26.766 us | 13.543 us | 1.98× |
| `fstree.resolve_entry` | depth-4 | 1 | 8.797 us | 4.660 us | 1.89× |
| `fstree.resolve_path` | depth-1 | 1 | 945.3 ns | 464.8 ns | 2.03× |
| `fstree.resolve_path` | depth-12 | 1 | 10.371 us | 5.230 us | 1.98× |
| `fstree.resolve_path` | depth-24 | 1 | 20.582 us | 10.539 us | 1.95× |
| `fstree.resolve_path` | depth-4 | 1 | 3.449 us | 1.812 us | 1.90× |
| `fstree.write_content` | file-corpus | 1 | 2.500 us | 36.500 us | 0.07× |
| `gc.close` | idle | 4 | 3.000 us | 1.000 us | 3.00× |
| `gc.open` | populated | 4 | 48.000 us | 32.000 us | 1.50× |
| `gc.prepare_ref` | missing-root | 4 | 2.922 us | 156.2 ns | 18.70× |
| `gc.prepare_ref` | tree-root/abort | 4 | 766.000 us | 1.238 ms | 0.62× |
| `gc.prepare_ref` | tree-root/commit | 4 | 649.000 us | 1.032 ms | 0.63× |
| `gc.release_ref` | batch | 4 | 7.3 ns | 1.2 ns | 6.00× |
| `gc.run` | nothing-to-reclaim | 4 | 228.000 us | 183.000 us | 1.25× |
| `gc.run` | reclaimable-packs | 4 | 823.000 us | 1.220 ms | 0.67× |
| `gc.status` | mark+score | 4 | 351.000 us | 217.000 us | 1.62× |
| `gc.why` | live-root | 4 | 52.000 us | 6.000 us | 8.67× |
| `gc.wipe` | store-reset | 4 | 460.000 us | 416.000 us | 1.11× |
| `inbox.close` | drained | 4 | 2.880 ms | 9.575 ms | 0.30× |
| `inbox.commit` | packs/duplicate | 4 | 20.875 us | 21.375 us | 0.98× |
| `inbox.discard` | packs | 4 | 20.500 us | 18.750 us | 1.09× |
| `inbox.drain` | packs/new | 4 | 1.632 ms | 1.751 ms | 0.93× |
| `inbox.open` | empty | 4 | 69.000 us | 112.000 us | 0.62× |
| `inbox.open` | sweeps-staged-tmp-files | 4 | 22.875 us | 30.875 us | 0.74× |
| `inbox.stage` | packs | 4 | 98.500 us | 93.375 us | 1.05× |
| `ingest.dir` | single-file/jobs-1 | 1 | 2.597 ms | 2.075 ms | 1.25× |
| `ingest.dir` | tree/fresh-store/jobs-1 | 1 | 439.785 us | 67.003 us | 6.56× |
| `ingest.dir` | tree/fresh-store/jobs-N | 4 | 502.504 us | 83.212 us | 6.04× |
| `ingest.dir` | tree/incremental-change/jobs-1 | 1 | 153.915 us | 32.038 us | 4.80× |
| `ingest.dir` | tree/incremental-change/jobs-N | 4 | 341.962 us | 46.245 us | 7.39× |
| `ingest.dir` | tree/unchanged-repeat/jobs-1 | 1 | 218.379 us | 35.173 us | 6.21× |
| `ingest.dir` | tree/unchanged-repeat/jobs-N | 4 | 276.522 us | 48.248 us | 5.73× |
| `ingest.objects` | tree/jobs-1 | 1 | 201.227 us | 29.663 us | 6.78× |
| `ingest.objects` | tree/jobs-N | 4 | 240.561 us | 46.451 us | 5.18× |
| `ingest.scan` | filtered/jobs-1 | 1 | 3.606 us | 5.570 us | 0.65× |
| `ingest.scan` | filtered/jobs-N | 4 | 3.385 us | 10.075 us | 0.34× |
| `ingest.scan` | unfiltered/jobs-1 | 1 | 2.555 us | 2.721 us | 0.94× |
| `key.accessors` | type+length+length_size+hash | 1 | 17.5 ns | 28.8 ns | 0.61× |
| `key.new` | 1MiB-duplicate | 1 | 222.000 us | 173.500 us | 1.28× |
| `key.new` | 1MiB-random | 1 | 257.000 us | 172.000 us | 1.49× |
| `key.new` | 1MiB-text | 1 | 253.000 us | 174.000 us | 1.45× |
| `key.new` | 4KiB-duplicate | 1 | 1.801 us | 1.328 us | 1.36× |
| `key.new` | 4KiB-random | 1 | 1.906 us | 1.316 us | 1.45× |
| `key.new` | 4KiB-text | 1 | 2.098 us | 1.324 us | 1.58× |
| `key.new` | empty | 1 | 117.5 ns | 80.0 ns | 1.47× |
| `key.new` | large-8MiB-duplicate | 1 | 2.406 ms | 1.399 ms | 1.72× |
| `key.new` | large-8MiB-random | 1 | 1.916 ms | 1.337 ms | 1.43× |
| `key.new` | large-8MiB-text | 1 | 1.916 ms | 1.391 ms | 1.38× |
| `key.new` | tiny-64B-duplicate | 1 | 133.9 ns | 83.9 ns | 1.60× |
| `key.new` | tiny-64B-random | 1 | 106.7 ns | 86.4 ns | 1.23× |
| `key.new` | tiny-64B-text | 1 | 108.9 ns | 86.0 ns | 1.27× |
| `key.new_from_hash` | batch | 1 | 55.0 ns | 40.0 ns | 1.38× |
| `key.parse` | canonical | 1 | 95.0 ns | 30.0 ns | 3.17× |
| `key.parse` | malformed | 1 | 77.1 ns | 2.0 ns | 39.50× |
| `key.string` | hex | 1 | 130.0 ns | 470.0 ns | 0.28× |
| `key.type_string` | names | 1 | 45.0 ns | 20.0 ns | 2.25× |
| `key.validate` | canonical | 1 | 15.0 ns | 5.0 ns | 3.00× |
| `packstore.append_record` | pre-encoded+sync | 1 | 9.859 us | 9.516 us | 1.04× |
| `packstore.barrier` | begin+observe+abort | 1 | 46.0 ns | 41.3 ns | 1.11× |
| `packstore.close` | populated | 1 | 160.000 us | 92.000 us | 1.74× |
| `packstore.compact` | 90-percent-dead | 1 | 878.7 ns | 1.115 us | 0.79× |
| `packstore.compact` | nothing-dead | 1 | 162.7 ns | 86.7 ns | 1.88× |
| `packstore.get` | hit | 1 | 8.010 us | 5.050 us | 1.59× |
| `packstore.get` | miss | 1 | 70.0 ns | 45.0 ns | 1.56× |
| `packstore.get_record` | hit | 1 | 520.0 ns | 320.0 ns | 1.62× |
| `packstore.has` | hit | 1 | 75.0 ns | 75.0 ns | 1.00× |
| `packstore.has` | miss | 1 | 40.0 ns | 30.0 ns | 1.33× |
| `packstore.has_outside` | sealed-segment | 1 | 60.0 ns | 40.0 ns | 1.50× |
| `packstore.liveness` | tenth-live | 1 | 35.000 us | 31.000 us | 1.13× |
| `packstore.mark_set_contains` | all-keys | 1 | 96.0 ns | 84.0 ns | 1.14× |
| `packstore.mark_set_mark` | all-keys | 1 | 62.7 ns | 46.0 ns | 1.36× |
| `packstore.missing` | half-present | 1 | 290.0 ns | 985.0 ns | 0.29× |
| `packstore.new_mark_set` | snapshot | 1 | 19.438 us | 12.375 us | 1.57× |
| `packstore.oldest_inflight_write` | idle | 1 | 14.9 ns | 3.2 ns | 4.69× |
| `packstore.open` | empty | 1 | 26.000 us | 14.000 us | 1.86× |
| `packstore.open` | populated-reopen | 1 | 217.000 us | 254.000 us | 0.85× |
| `packstore.put` | 1MiB-duplicate/sync-off | 1 | 669.000 us | 234.000 us | 2.86× |
| `packstore.put` | 1MiB-random/sync-off | 1 | 978.000 us | 433.000 us | 2.26× |
| `packstore.put` | 1MiB-text/sync-off | 1 | 3.891 ms | 1.862 ms | 2.09× |
| `packstore.put` | 4KiB-duplicate/sync-off | 1 | 2.500 us | 1.500 us | 1.67× |
| `packstore.put` | 4KiB-random/already-present | 1 | 218.8 ns | 93.8 ns | 2.33× |
| `packstore.put` | 4KiB-random/sync-off | 1 | 14.906 us | 8.000 us | 1.86× |
| `packstore.put` | 4KiB-random/sync-on | 1 | 28.844 us | 22.375 us | 1.29× |
| `packstore.put` | 4KiB-text/sync-off | 1 | 51.375 us | 16.656 us | 3.08× |
| `packstore.put` | empty/sync-off | 1 | 163.8 ns | 118.8 ns | 1.38× |
| `packstore.put` | large-8MiB-duplicate/sync-off | 1 | 8.496 ms | 2.034 ms | 4.18× |
| `packstore.put` | large-8MiB-random/sync-off | 1 | 7.590 ms | 3.643 ms | 2.08× |
| `packstore.put` | large-8MiB-text/sync-off | 1 | 26.733 ms | 14.627 ms | 1.83× |
| `packstore.put` | tiny-64B-duplicate/sync-off | 1 | 115.7 ns | 99.6 ns | 1.16× |
| `packstore.put` | tiny-64B-random/sync-off | 1 | 2.714 us | 1.723 us | 1.58× |
| `packstore.put` | tiny-64B-text/sync-off | 1 | 2.482 us | 2.283 us | 1.09× |
| `packstore.put_verified` | already-intact | 1 | 81.200 us | 52.040 us | 1.56× |
| `packstore.record` | by-location | 1 | 855.0 ns | 835.0 ns | 1.02× |
| `packstore.remove` | one-sealed-segment | 1 | 112.000 us | 99.000 us | 1.13× |
| `packstore.scan_index` | all-segments | 1 | 34.0 ns | 24.0 ns | 1.42× |
| `packstore.segments` | list | 1 | 2.797 us | 2.094 us | 1.34× |
| `packstore.sort_by_location` | scattered | 1 | 195.0 ns | 110.0 ns | 1.77× |
| `packstore.stored_size` | hit | 1 | 100.0 ns | 55.0 ns | 1.82× |
| `packstore.verify` | full-scrub | 1 | 7.727 us | 4.817 us | 1.60× |
| `packstore.wipe` | populated | 1 | 257.000 us | 226.000 us | 1.14× |
| `packstore.write_batch` | mixed-objects | 1 | 33.546 us | 21.669 us | 1.55× |
| `packstore.write_parallel` | duplicate-stream/writers-N | 4 | 382.0 ns | 2.073 us | 0.18× |
| `packstore.write_parallel` | mixed-objects/writers-1/verify-false | 1 | 35.014 us | 23.423 us | 1.49× |
| `packstore.write_parallel` | mixed-objects/writers-1/verify-true | 1 | 36.055 us | 24.305 us | 1.48× |
| `packstore.write_parallel` | mixed-objects/writers-N/verify-false | 4 | 34.498 us | 24.003 us | 1.44× |
| `packstore.write_parallel` | mixed-objects/writers-N/verify-true | 4 | 36.683 us | 25.729 us | 1.43× |
| `reference.decode` | signed | 1 | 875.0 ns | 445.3 ns | 1.96× |
| `reference.encode` | signed | 1 | 277.3 ns | 175.8 ns | 1.58× |
| `reference.signature_payload` | signed | 1 | 261.7 ns | 171.9 ns | 1.52× |
| `reference.validate_name` | valid | 1 | 78.1 ns | 31.2 ns | 2.50× |
| `reference.validate_user` | valid | 1 | 82.0 ns | 101.6 ns | 0.81× |
| `refstore.all` | records | 1 | 510.0 ns | 195.0 ns | 2.62× |
| `refstore.close` | populated | 1 | 82.000 us | 766.000 us | 0.11× |
| `refstore.delete` | records | 1 | 1.680 us | 20.825 us | 0.08× |
| `refstore.get` | hit | 1 | 1.135 us | 440.0 ns | 2.58× |
| `refstore.get` | miss | 1 | 260.0 ns | 325.0 ns | 0.80× |
| `refstore.open` | empty | 1 | 394.000 us | 996.000 us | 0.40× |
| `refstore.open` | populated-reopen | 1 | 760.000 us | 350.000 us | 2.17× |
| `refstore.put` | records/sync-false | 1 | 395.0 ns | 21.150 us | 0.02× |
| `refstore.put` | records/sync-true | 1 | 11.480 us | 21.030 us | 0.55× |
| `refstore.put_batch` | records/sync-false | 1 | 130.0 ns | 815.0 ns | 0.16× |
| `refstore.wipe` | records | 1 | 140.000 us | 341.000 us | 0.41× |
| `tarexport.write` | fixture-tree | 1 | 2.609 us | 3.427 us | 0.76× |
| `tarextract.extract` | fixture-tree | 1 | 40.797 us | 36.558 us | 1.12× |

## Encoded sizes (per core, never divided)

The two cores compress record payloads with different encoders —
`klauspost/compress` in Go, libzstd in Rust — so the same input legitimately
becomes a different number of bytes. Those sizes are side by side here and
are never used as a shared denominator; the throughput figures above divide
by the *input*, which is byte-identical.

| Operation | Workload | Items | Input | Go encoded | Rust encoded | Go/Rust |
|---|---|---:|---:|---:|---:|---:|
| `amberpack.encode_record` | 1MiB-duplicate | 2 | 2.00 MiB | 2.00 MiB | 2.00 MiB | 1.000× |
| `amberpack.encode_record` | 1MiB-random | 1 | 1.00 MiB | 1.00 MiB | 1.00 MiB | 1.000× |
| `amberpack.encode_record` | 1MiB-text | 1 | 1.00 MiB | 196.60 KiB | 195.32 KiB | 1.007× |
| `amberpack.encode_record` | 4KiB-duplicate | 256 | 1.00 MiB | 1.01 MiB | 1.01 MiB | 1.000× |
| `amberpack.encode_record` | 4KiB-random | 256 | 1.00 MiB | 1.01 MiB | 1.01 MiB | 1.000× |
| `amberpack.encode_record` | 4KiB-text | 256 | 1.00 MiB | 300.79 KiB | 301.77 KiB | 0.997× |
| `amberpack.encode_record` | empty | 800 | 0 B | 35.94 KiB | 35.94 KiB | 1.000× |
| `amberpack.encode_record` | large-8MiB-duplicate | 2 | 16.00 MiB | 16.00 MiB | 16.00 MiB | 1.000× |
| `amberpack.encode_record` | large-8MiB-random | 1 | 8.00 MiB | 8.00 MiB | 8.00 MiB | 1.000× |
| `amberpack.encode_record` | large-8MiB-text | 1 | 8.00 MiB | 1.53 MiB | 1.52 MiB | 1.007× |
| `amberpack.encode_record` | tiny-64B-duplicate | 16,384 | 1.00 MiB | 1.72 MiB | 1.72 MiB | 1.000× |
| `amberpack.encode_record` | tiny-64B-random | 16,384 | 1.00 MiB | 1.72 MiB | 1.72 MiB | 1.000× |
| `amberpack.encode_record` | tiny-64B-text | 16,384 | 1.00 MiB | 1.72 MiB | 1.72 MiB | 1.001× |
| `amberpack.writer_add` | mixed-objects | 64 | 2.07 MiB | 1.23 MiB | 1.23 MiB | 1.002× |

## Shared wire inputs

A decoder measured against its own encoder's output is measured against a
different input in each core. These packs are produced once, before anything
is measured, and **both** drivers read **both** of them: every reader and
decoder case runs once per producer, over byte-identical input.

| Producer | Objects | Encoded size | SHA-256 |
|---|---:|---:|---|
| go | 64 | 1.23 MiB | `3d3d567b71036e7aad778cf2250318de6e3b51c32657936df24e9f0027383a0c` |
| rust | 64 | 1.23 MiB | `dcee6d4fecaba508a3bd4d89c5df1c3727c6ed8c54d25fdb2aac1d2b64f33013` |

## Single-core operations

These have no counterpart in the other core, which declares them unsupported
with a reason. They carry a measurement from one side only and are never
compared. See `COVERAGE.md`.

| Core | Operation | Workload | n | ns/op | var |
|---|---|---|---:|---:|---:|
| go | `gc.begin_write` | gate-span | 1 | 32.3 ns | — |
| rust | `binaryfuse.contains` | store-keys/rust-only | 1 | 2.9 ns | — |
| rust | `binaryfuse.new` | store-keys/rust-only | 1 | 16.1 ns | — |
| rust | `binaryfuse.parse_section` | store-keys/rust-only | 1 | 139.0 ns | — |
| rust | `binaryfuse.section_bytes` | store-keys/rust-only | 1 | 161.6 ns | — |
| rust | `cbor.head_primitives` | append+read/rust-only | 1 | 5.0 ns | — |

## Runtime counters (not comparable)

The Go runtime accounts for heap allocation; the Rust driver deliberately
runs with an uninstrumented allocator, because counting allocations there
would have cost time inside the very intervals being measured. These numbers
are therefore **Go-only** and are never used in a comparison.

| Operation | Workload | Counter | Median |
|---|---|---|---:|
| `ingest.dir` | tree/incremental-change/jobs-N | go.heap_alloc_bytes | 728,040,832 |
| `ingest.dir` | tree/incremental-change/jobs-1 | go.heap_alloc_bytes | 727,992,560 |
| `ingest.dir` | tree/fresh-store/jobs-N | go.heap_alloc_bytes | 727,165,080 |
| `ingest.dir` | tree/fresh-store/jobs-1 | go.heap_alloc_bytes | 727,098,464 |
| `ingest.dir` | tree/unchanged-repeat/jobs-N | go.heap_alloc_bytes | 711,034,224 |
| `ingest.dir` | tree/unchanged-repeat/jobs-1 | go.heap_alloc_bytes | 710,953,352 |
| `ingest.objects` | tree/jobs-N | go.heap_alloc_bytes | 710,923,952 |
| `ingest.objects` | tree/jobs-1 | go.heap_alloc_bytes | 710,901,216 |
| `fstree.encode_file_node` | children-65536 | go.heap_alloc_bytes | 394,793,848 |
| `fstree.decode_file_node` | children-65536 | go.heap_alloc_bytes | 369,101,824 |
| `amberpack.reader_all` | truncated-stream/producer-rust | go.heap_alloc_bytes | 115,540,888 |
| `amberpack.reader_all` | truncated-stream/producer-go | go.heap_alloc_bytes | 115,536,424 |
| `fstree.list_entries` | width-40000/full-paging-100 | go.heap_alloc_bytes | 59,303,408 |
| `amberpack.encode_record` | large-8MiB-duplicate | go.heap_alloc_bytes | 54,558,720 |
| `fstree.collect_entries` | width-40000 | go.heap_alloc_bytes | 42,204,320 |
| `fstree.check_complete` | widest/jobs-N | go.heap_alloc_bytes | 36,579,976 |
| `fstree.check_complete` | widest/jobs-1 | go.heap_alloc_bytes | 36,575,656 |
| `fstree.check_complete` | width-40000/jobs-1 | go.heap_alloc_bytes | 36,575,656 |
| `fstree.dir_builder` | entries-40000 | go.heap_alloc_bytes | 32,885,040 |
| `fstree.reachable_keys` | width-40000 | go.heap_alloc_bytes | 31,771,528 |
| `packstore.put` | large-8MiB-duplicate/sync-off | go.heap_alloc_bytes | 27,288,208 |
| `packstore.put` | large-8MiB-random/sync-off | go.heap_alloc_bytes | 27,288,208 |
| `amberpack.encode_record` | large-8MiB-random | go.heap_alloc_bytes | 27,279,360 |
| `packstore.write_batch` | mixed-objects | go.heap_alloc_bytes | 24,391,840 |
| `packstore.write_parallel` | mixed-objects/writers-N/verify-false | go.heap_alloc_bytes | 24,348,576 |

The full set (496 rows) is in `counters.csv`.

## Resident set

`getrusage` reports one number per process: the high-water resident set over
the **whole lifetime of the driver**, which only ever grows. It is not a
per-operation peak and it cannot be attributed to a case — a case measured
after a large fixture was built inherits that fixture's high-water mark. The
only honest reading is the one below: how much memory each driver had touched
by the end of its run.

* Go driver, end of run: 637,140 KiB
* Rust driver, end of run: 323,016 KiB

The per-sample values are in `samples.csv` for completeness, under the same
caveat.

## Charts

### amberignore-1

![amberignore-1](plots/amberignore-1.svg)

### amberignore-2

![amberignore-2](plots/amberignore-2.svg)

### amberpack-1

![amberpack-1](plots/amberpack-1.svg)

### amberpack-2

![amberpack-2](plots/amberpack-2.svg)

### cbor

![cbor](plots/cbor.svg)

### chunkers-1

![chunkers-1](plots/chunkers-1.svg)

### chunkers-2

![chunkers-2](plots/chunkers-2.svg)

### fstree-1

![fstree-1](plots/fstree-1.svg)

### fstree-2

![fstree-2](plots/fstree-2.svg)

### fstree-3

![fstree-3](plots/fstree-3.svg)

### fstree-4

![fstree-4](plots/fstree-4.svg)

### fstree-5

![fstree-5](plots/fstree-5.svg)

### gc-1

![gc-1](plots/gc-1.svg)

### gc-2

![gc-2](plots/gc-2.svg)

### gc-3

![gc-3](plots/gc-3.svg)

### inbox

![inbox](plots/inbox.svg)

### ingest-1

![ingest-1](plots/ingest-1.svg)

### ingest-2

![ingest-2](plots/ingest-2.svg)

### key-1

![key-1](plots/key-1.svg)

### key-2

![key-2](plots/key-2.svg)

### key-3

![key-3](plots/key-3.svg)

### packstore-1

![packstore-1](plots/packstore-1.svg)

### packstore-2

![packstore-2](plots/packstore-2.svg)

### packstore-3

![packstore-3](plots/packstore-3.svg)

### reference

![reference](plots/reference.svg)

### refstore-1

![refstore-1](plots/refstore-1.svg)

### refstore-2

![refstore-2](plots/refstore-2.svg)

### tar

![tar](plots/tar.svg)

### scaling-amberpack-encode_record-item_bytes-duplicate

![scaling-amberpack-encode_record-item_bytes-duplicate](plots/scaling-amberpack-encode_record-item_bytes-duplicate.svg)

### scaling-amberpack-encode_record-item_bytes-random

![scaling-amberpack-encode_record-item_bytes-random](plots/scaling-amberpack-encode_record-item_bytes-random.svg)

### scaling-amberpack-encode_record-item_bytes-text

![scaling-amberpack-encode_record-item_bytes-text](plots/scaling-amberpack-encode_record-item_bytes-text.svg)

### scaling-fstree-check_complete-width-plain

![scaling-fstree-check_complete-width-plain](plots/scaling-fstree-check_complete-width-plain.svg)

### scaling-fstree-collect_entries-width-plain

![scaling-fstree-collect_entries-width-plain](plots/scaling-fstree-collect_entries-width-plain.svg)

### scaling-fstree-decode_dir_leaf-entries-plain

![scaling-fstree-decode_dir_leaf-entries-plain](plots/scaling-fstree-decode_dir_leaf-entries-plain.svg)

### scaling-fstree-decode_dir_node-entries-plain

![scaling-fstree-decode_dir_node-entries-plain](plots/scaling-fstree-decode_dir_node-entries-plain.svg)

### scaling-fstree-decode_file_node-entries-plain

![scaling-fstree-decode_file_node-entries-plain](plots/scaling-fstree-decode_file_node-entries-plain.svg)

### scaling-fstree-dir_builder-entries-plain

![scaling-fstree-dir_builder-entries-plain](plots/scaling-fstree-dir_builder-entries-plain.svg)

### scaling-fstree-encode_blob-item_bytes-duplicate

![scaling-fstree-encode_blob-item_bytes-duplicate](plots/scaling-fstree-encode_blob-item_bytes-duplicate.svg)

### scaling-fstree-encode_blob-item_bytes-random

![scaling-fstree-encode_blob-item_bytes-random](plots/scaling-fstree-encode_blob-item_bytes-random.svg)

### scaling-fstree-encode_blob-item_bytes-text

![scaling-fstree-encode_blob-item_bytes-text](plots/scaling-fstree-encode_blob-item_bytes-text.svg)

### scaling-fstree-encode_dir_leaf-entries-plain

![scaling-fstree-encode_dir_leaf-entries-plain](plots/scaling-fstree-encode_dir_leaf-entries-plain.svg)

### scaling-fstree-encode_dir_node-entries-plain

![scaling-fstree-encode_dir_node-entries-plain](plots/scaling-fstree-encode_dir_node-entries-plain.svg)

### scaling-fstree-encode_file_node-entries-plain

![scaling-fstree-encode_file_node-entries-plain](plots/scaling-fstree-encode_file_node-entries-plain.svg)

### scaling-fstree-index_builder_file-entries-plain

![scaling-fstree-index_builder_file-entries-plain](plots/scaling-fstree-index_builder_file-entries-plain.svg)

### scaling-fstree-list_entries-width-plain

![scaling-fstree-list_entries-width-plain](plots/scaling-fstree-list_entries-width-plain.svg)

### scaling-fstree-lookup_entry-width-hit

![scaling-fstree-lookup_entry-width-hit](plots/scaling-fstree-lookup_entry-width-hit.svg)

### scaling-fstree-lookup_entry-width-miss

![scaling-fstree-lookup_entry-width-miss](plots/scaling-fstree-lookup_entry-width-miss.svg)

### scaling-fstree-reachable_keys-width-plain

![scaling-fstree-reachable_keys-width-plain](plots/scaling-fstree-reachable_keys-width-plain.svg)

### scaling-fstree-resolve_entry-depth-plain

![scaling-fstree-resolve_entry-depth-plain](plots/scaling-fstree-resolve_entry-depth-plain.svg)

### scaling-fstree-resolve_path-depth-plain

![scaling-fstree-resolve_path-depth-plain](plots/scaling-fstree-resolve_path-depth-plain.svg)

### scaling-key-new-item_bytes-duplicate

![scaling-key-new-item_bytes-duplicate](plots/scaling-key-new-item_bytes-duplicate.svg)

### scaling-key-new-item_bytes-random

![scaling-key-new-item_bytes-random](plots/scaling-key-new-item_bytes-random.svg)

### scaling-key-new-item_bytes-text

![scaling-key-new-item_bytes-text](plots/scaling-key-new-item_bytes-text.svg)

### scaling-packstore-put-item_bytes-duplicate

![scaling-packstore-put-item_bytes-duplicate](plots/scaling-packstore-put-item_bytes-duplicate.svg)

### scaling-packstore-put-item_bytes-text

![scaling-packstore-put-item_bytes-text](plots/scaling-packstore-put-item_bytes-text.svg)

## Reproducing

```sh
./run.sh --profile quick --seed 1592639710 --cpus 8
```

Raw data next to this file: `go-pass*.json`, `rust-pass*.json` (per-repetition
samples, one document per measurement pass), `samples.csv`, `summary.csv`,
`paired.csv`, `scaling.csv`, `checks.csv`, `counters.csv`, `encodings.csv`,
`unsupported.csv`, `report.json`.

## Scope limits

* Measurements are warm. Nothing here says anything about cold-cache behaviour.
* 1 repetitions on one host. The dispersion columns are the honest
  bound on what that supports; differences of the order of the variation column
  are not differences.
* `refstore` is Pebble in Go and redb in Rust: the operation is the same, the
  storage engine is not, and a directory written by one is not readable by the
  other. Its rows compare two designs, not two implementations of one design.
* zstd-compressed record payloads come from `klauspost/compress` in Go and
  libzstd in Rust. Records, segment bodies and wire packs are therefore
  interoperable but not byte-identical; the decode cases are measured on shared
  inputs and the encoders' output sizes are reported separately.
* Which sealed segment a given object lands in follows the compressed sizes, so
  per-segment counts differ between the cores by construction. The operations
  that would otherwise be sensitive to that (`packstore.scan_index`) cover every
  segment rather than one.
* Operations that pick their own parallelism are run under one shared CPU set so
  both cores see the same bound; the effective width is recorded above.
* The measured interval contains the core call and a constant-time consumption of
  its output, and nothing else. Fixture construction, store copies and the
  full-output digests that prove the two cores agree are all outside it.
* The per-case CPU figures come from `getrusage` for the whole process.
