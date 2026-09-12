# Amber core operations: Go vs Rust (standard profile)

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
* Profile `standard`: 7 measured repetitions after 2 warm-up passes, seed `1592639710`.
* The repetitions were measured in 2 passes of opposite core order (Go first, then Rust first), so neither core was systematically measured on the colder machine.
* Fixtures: 64 MiB corpora, 1200 tree files, 4000 wide entries, depth 24, 30000 store objects, 16 MiB segments, 5000 references, 48 inbox packs.
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
* every case has exactly repetitions 0..6 on both sides, each once
* every repetition of a case describes the same work as its siblings
* all 247 shared cases agree on threads, operation count, byte count and every workload dimension
* a case measured by one core only belongs to an operation the other core explicitly declares unsupported
* no core both declares an operation unsupported and measures it, and every declaration carries a reason
* every repetition of each of the 243 stable cases produced the same output
* the two cores produced byte-identical output in all 160 cases that require it
* both cores decoded the same bytes: 2 wire inputs, identical content hashes
* the two cores encoded the same input in all 14 recorded encodings; their output sizes are reported separately and never divided
* the measured set is exactly the manifest's: 100 paired operations over 247 workloads, and every check it names was recorded
* the repetitions were measured in 2 passes of opposite core order, so neither core was systematically measured first

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

> With 7 repetitions the interval is wide on purpose and the *p*
> value is a weak instrument. A ratio whose interval spans 1 is not evidence
> of a difference, and none of these numbers supports a claim about the two
> cores in general.

### `amberignore`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `amberignore.descend` | 8-subdirs | 1 | 7 | 5.091 us | 1.6% | 3.156 us | 0.5% | 1.61× | 1.59–1.66 | 0.0006 |
| `amberignore.ignored` | mixed-names | 1 | 7 | 210.8 ns | 2.0% | 118.5 ns | 2.5% | 1.78× | 1.71–1.95 | 0.0006 |
| `amberignore.ignored` | nil-matcher | 1 | 7 | 5.1 ns | 1.5% | 0.1 ns | 0.0% | 57.78× | 52.00–96.40 | 0.0019 |
| `amberignore.root` | load-root | 1 | 7 | 7.317 us | 0.9% | 4.282 us | 1.1% | 1.71× | 1.70–1.78 | 0.0006 |

### `amberpack`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `amberpack.decode_payload` | mixed-records/producer-go | 1 | 7 | 15.494 us | 3.6% | 10.679 us | 0.7% | 1.45× | 1.40–1.57 | 0.0006 |
| `amberpack.decode_payload` | mixed-records/producer-rust | 1 | 7 | 14.522 us | 1.6% | 9.770 us | 0.3% | 1.49× | 1.41–1.51 | 0.0006 |
| `amberpack.encode_record` | 1MiB-duplicate | 1 | 7 | 439.512 us | 9.9% | 259.699 us | 0.5% | 1.69× | 1.48–1.87 | 0.0006 |
| `amberpack.encode_record` | 1MiB-random | 1 | 7 | 384.344 us | 3.6% | 258.148 us | 0.1% | 1.49× | 1.45–1.57 | 0.0006 |
| `amberpack.encode_record` | 1MiB-text | 1 | 7 | 3.175 ms | 1.2% | 1.778 ms | 1.0% | 1.79× | 1.74–1.82 | 0.0006 |
| `amberpack.encode_record` | 4KiB-duplicate | 1 | 7 | 3.492 us | 5.0% | 4.399 us | 1.1% | 0.79× | 0.75–0.90 | 0.0006 |
| `amberpack.encode_record` | 4KiB-random | 1 | 7 | 3.668 us | 3.0% | 4.500 us | 3.5% | 0.82× | 0.76–0.89 | 0.0006 |
| `amberpack.encode_record` | 4KiB-text | 1 | 7 | 24.287 us | 1.0% | 14.217 us | 0.1% | 1.71× | 1.69–1.72 | 0.0006 |
| `amberpack.encode_record` | empty | 1 | 7 | 62.0 ns | 1.4% | 290.1 ns | 0.5% | 0.21× | 0.21–0.22 | 0.0006 |
| `amberpack.encode_record` | large-8MiB-duplicate | 1 | 7 | 3.811 ms | 2.4% | 2.072 ms | 0.2% | 1.84× | 1.80–1.94 | 0.0006 |
| `amberpack.encode_record` | large-8MiB-random | 1 | 7 | 3.668 ms | 1.8% | 2.077 ms | 0.7% | 1.77× | 1.76–1.99 | 0.0006 |
| `amberpack.encode_record` | large-8MiB-text | 1 | 7 | 25.683 ms | 1.2% | 14.086 ms | 0.2% | 1.82× | 1.80–1.90 | 0.0006 |
| `amberpack.encode_record` | tiny-64B-duplicate | 1 | 7 | 513.5 ns | 0.8% | 619.0 ns | 0.9% | 0.83× | 0.78–0.84 | 0.0006 |
| `amberpack.encode_record` | tiny-64B-random | 1 | 7 | 562.2 ns | 2.7% | 622.3 ns | 1.0% | 0.90× | 0.85–0.93 | 0.0006 |
| `amberpack.encode_record` | tiny-64B-text | 1 | 7 | 1.135 us | 0.6% | 1.161 us | 0.1% | 0.98× | 0.97–0.99 | 0.0111 |
| `amberpack.parse_record` | corrupt-crc/producer-go | 1 | 7 | 131.6 ns | 4.0% | 102.1 ns | 0.2% | 1.29× | 1.24–1.39 | 0.0021 |
| `amberpack.parse_record` | corrupt-crc/producer-rust | 1 | 7 | 129.2 ns | 1.8% | 101.9 ns | 0.1% | 1.27× | 1.24–1.32 | 0.0021 |
| `amberpack.parse_record` | mixed-records/producer-go | 1 | 7 | 723.8 ns | 1.2% | 2.275 us | 0.2% | 0.32× | 0.30–0.33 | 0.0006 |
| `amberpack.parse_record` | mixed-records/producer-rust | 1 | 7 | 714.3 ns | 1.1% | 2.269 us | 0.0% | 0.31× | 0.31–0.33 | 0.0006 |
| `amberpack.reader_all` | mixed-objects/producer-go | 1 | 7 | 16.479 us | 0.8% | 13.581 us | 1.2% | 1.21× | 1.19–1.30 | 0.0006 |
| `amberpack.reader_all` | mixed-objects/producer-rust | 1 | 7 | 16.055 us | 2.3% | 12.705 us | 0.8% | 1.26× | 1.23–1.36 | 0.0006 |
| `amberpack.reader_all` | truncated-stream/producer-go | 1 | 7 | 5.269 ms | 5.5% | 3.280 ms | 0.1% | 1.61× | 1.46–1.63 | 0.0006 |
| `amberpack.reader_all` | truncated-stream/producer-rust | 1 | 7 | 5.069 ms | 0.7% | 3.078 ms | 0.1% | 1.65× | 1.63–1.67 | 0.0006 |
| `amberpack.reader_records` | mixed-objects/producer-go | 1 | 7 | 1.472 us | 3.3% | 2.829 us | 0.2% | 0.52× | 0.51–0.77 | 0.0006 |
| `amberpack.reader_records` | mixed-objects/producer-rust | 1 | 7 | 1.438 us | 1.1% | 2.815 us | 0.2% | 0.51× | 0.50–0.54 | 0.0006 |
| `amberpack.writer_add` | mixed-objects | 1 | 7 | 67.028 us | 1.2% | 39.636 us | 0.2% | 1.69× | 1.66–1.71 | 0.0006 |
| `amberpack.writer_add_record` | pre-encoded-records/producer-go | 1 | 7 | 29.1 ns | 1.5% | 7.6 ns | 2.7% | 3.83× | 3.71–5.11 | 0.0006 |
| `amberpack.writer_add_record` | pre-encoded-records/producer-rust | 1 | 7 | 29.5 ns | 2.1% | 7.7 ns | 3.9% | 3.82× | 3.73–4.77 | 0.0006 |

### `cbor`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `cbor.decode_xattrs` | xattrs-3 | 1 | 7 | 409.8 ns | 4.2% | 173.3 ns | 0.4% | 2.36× | 2.23–2.47 | 0.0006 |
| `cbor.decode_xattrs` | xattrs-64 | 1 | 7 | 8.915 us | 1.0% | 5.565 us | 0.7% | 1.60× | 1.56–1.63 | 0.0006 |
| `cbor.encode_xattrs` | xattrs-3 | 1 | 7 | 444.3 ns | 2.2% | 252.5 ns | 4.5% | 1.76× | 1.69–1.91 | 0.0006 |
| `cbor.encode_xattrs` | xattrs-64 | 1 | 7 | 25.173 us | 3.2% | 4.513 us | 0.7% | 5.58× | 5.31–5.79 | 0.0006 |

### `chunkers`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `chunkers.item_chunker` | is_boundary/bits-7 | 1 | 7 | 52.7 ns | 1.0% | 32.5 ns | 0.1% | 1.62× | 1.58–1.64 | 0.0006 |
| `chunkers.new_item_chunker` | bits-4..12 | 1 | 7 | 13.1 ns | 0.4% | 13.0 ns | 0.1% | 1.01× | 1.01–1.02 | 0.0293 |
| `chunkers.split_bytes` | compressible/4-16-64KiB | 1 | 7 | 56.309 us | 0.3% | 40.272 us | 0.4% | 1.40× | 1.38–1.41 | 0.0006 |
| `chunkers.split_bytes` | compressible/default-sizes | 1 | 7 | 897.013 us | 0.4% | 660.511 us | 0.0% | 1.36× | 1.35–1.37 | 0.0006 |
| `chunkers.split_bytes` | random/default-sizes | 1 | 7 | 49.214 us | 0.7% | 44.529 us | 0.8% | 1.11× | 1.09–1.12 | 0.0006 |
| `chunkers.split_bytes` | tiny-64KiB/default-sizes | 1 | 7 | 49.905 us | 2.0% | 22.133 us | 0.5% | 2.25× | 2.23–2.38 | 0.0006 |

### `fstree`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `fstree.check_complete` | incomplete/jobs-1 | 1 | 7 | 32.412 us | 6.7% | 6.001 us | 3.5% | 5.40× | 4.68–6.03 | 0.0006 |
| `fstree.check_complete` | widest/jobs-1 | 1 | 7 | 668.5 ns | 0.7% | 272.5 ns | 0.1% | 2.45× | 2.43–2.48 | 0.0006 |
| `fstree.check_complete` | widest/jobs-N | 8 | 7 | 350.7 ns | 1.5% | 129.3 ns | 2.5% | 2.71× | 2.56–2.82 | 0.0006 |
| `fstree.check_complete` | width-16/jobs-1 | 1 | 7 | 1.548 us | 11.7% | 313.5 ns | 2.1% | 4.94× | 4.45–6.20 | 0.0006 |
| `fstree.check_complete` | width-256/jobs-1 | 1 | 7 | 751.5 ns | 1.9% | 268.4 ns | 1.1% | 2.80× | 2.42–2.85 | 0.0006 |
| `fstree.check_complete` | width-40000/jobs-1 | 1 | 7 | 670.7 ns | 0.8% | 273.4 ns | 0.2% | 2.45× | 2.43–2.48 | 0.0006 |
| `fstree.check_complete` | width-4096/jobs-1 | 1 | 7 | 661.9 ns | 0.7% | 276.2 ns | 0.3% | 2.40× | 2.38–2.46 | 0.0006 |
| `fstree.child_keys` | dir-leaf-128 | 1 | 7 | 46.673 us | 0.8% | 29.050 us | 0.3% | 1.61× | 1.58–1.62 | 0.0006 |
| `fstree.child_keys` | dir-node-128 | 1 | 7 | 21.196 us | 1.2% | 14.180 us | 0.5% | 1.49× | 1.47–1.52 | 0.0006 |
| `fstree.child_keys` | file-node-1024 | 1 | 7 | 88.033 us | 0.8% | 71.504 us | 0.2% | 1.23× | 1.22–1.24 | 0.0006 |
| `fstree.collect_entries` | width-16 | 1 | 7 | 677.5 ns | 6.7% | 242.3 ns | 2.3% | 2.80× | 2.48–2.98 | 0.0006 |
| `fstree.collect_entries` | width-256 | 1 | 7 | 379.0 ns | 0.5% | 219.8 ns | 0.7% | 1.72× | 1.69–1.77 | 0.0006 |
| `fstree.collect_entries` | width-40000 | 1 | 7 | 397.1 ns | 0.6% | 221.3 ns | 0.5% | 1.79× | 1.73–1.82 | 0.0006 |
| `fstree.collect_entries` | width-4096 | 1 | 7 | 365.5 ns | 0.8% | 223.8 ns | 0.4% | 1.63× | 1.63–1.66 | 0.0006 |
| `fstree.decode_dir_leaf` | entries-1024 | 1 | 7 | 350.8 ns | 0.7% | 215.4 ns | 0.0% | 1.63× | 1.62–1.65 | 0.0006 |
| `fstree.decode_dir_leaf` | entries-128 | 1 | 7 | 356.2 ns | 0.4% | 212.8 ns | 0.2% | 1.67× | 1.67–1.70 | 0.0006 |
| `fstree.decode_dir_leaf` | entries-128-partial-change | 1 | 7 | 370.9 ns | 0.5% | 223.1 ns | 0.6% | 1.66× | 1.64–1.68 | 0.0006 |
| `fstree.decode_dir_leaf` | entries-128-with-xattrs | 1 | 7 | 370.2 ns | 0.2% | 223.7 ns | 0.3% | 1.66× | 1.65–1.79 | 0.0006 |
| `fstree.decode_dir_leaf` | entries-8 | 1 | 7 | 374.3 ns | 0.7% | 215.4 ns | 1.2% | 1.74× | 1.69–1.78 | 0.0006 |
| `fstree.decode_dir_node` | pairs-1024 | 1 | 7 | 169.3 ns | 0.5% | 99.5 ns | 0.3% | 1.70× | 1.69–1.71 | 0.0006 |
| `fstree.decode_dir_node` | pairs-128 | 1 | 7 | 167.5 ns | 0.9% | 96.3 ns | 0.5% | 1.74× | 1.71–1.76 | 0.0006 |
| `fstree.decode_dir_node` | pairs-8 | 1 | 7 | 187.6 ns | 1.0% | 85.9 ns | 0.5% | 2.18× | 2.14–2.22 | 0.0006 |
| `fstree.decode_file_node` | children-1024 | 1 | 7 | 86.0 ns | 0.6% | 69.0 ns | 0.4% | 1.25× | 1.23–1.32 | 0.0006 |
| `fstree.decode_file_node` | children-128 | 1 | 7 | 87.4 ns | 1.0% | 63.0 ns | 0.2% | 1.39× | 1.37–1.43 | 0.0006 |
| `fstree.decode_file_node` | children-65536 | 1 | 7 | 89.0 ns | 0.1% | 69.8 ns | 0.1% | 1.28× | 1.26–1.28 | 0.0006 |
| `fstree.decode_file_node` | children-8 | 1 | 7 | 110.5 ns | 2.0% | 63.1 ns | 0.9% | 1.75× | 1.71–1.81 | 0.0006 |
| `fstree.dir_builder` | entries-16 | 1 | 7 | 1.363 us | 9.1% | 333.1 ns | 3.2% | 4.09× | 3.69–6.75 | 0.0006 |
| `fstree.dir_builder` | entries-256 | 1 | 7 | 545.6 ns | 0.9% | 334.6 ns | 0.6% | 1.63× | 1.62–1.67 | 0.0021 |
| `fstree.dir_builder` | entries-40000 | 1 | 7 | 496.6 ns | 2.7% | 332.9 ns | 0.2% | 1.49× | 1.45–1.51 | 0.0006 |
| `fstree.dir_builder` | entries-4096 | 1 | 7 | 482.2 ns | 1.1% | 320.1 ns | 1.2% | 1.51× | 1.49–1.55 | 0.0006 |
| `fstree.encode_blob` | 1MiB-duplicate | 1 | 7 | 192.751 us | 0.8% | 135.028 us | 0.4% | 1.43× | 1.41–1.44 | 0.0006 |
| `fstree.encode_blob` | 1MiB-random | 1 | 7 | 202.469 us | 5.2% | 134.101 us | 0.3% | 1.51× | 1.43–1.59 | 0.0006 |
| `fstree.encode_blob` | 1MiB-text | 1 | 7 | 205.398 us | 6.6% | 135.251 us | 0.7% | 1.52× | 1.37–1.59 | 0.0006 |
| `fstree.encode_blob` | 4KiB-duplicate | 1 | 7 | 1.786 us | 0.7% | 1.122 us | 0.2% | 1.59× | 1.58–1.69 | 0.0006 |
| `fstree.encode_blob` | 4KiB-random | 1 | 7 | 1.817 us | 1.0% | 1.121 us | 0.2% | 1.62× | 1.61–1.65 | 0.0006 |
| `fstree.encode_blob` | 4KiB-text | 1 | 7 | 1.916 us | 4.1% | 1.127 us | 0.7% | 1.70× | 1.50–1.74 | 0.0006 |
| `fstree.encode_blob` | empty | 1 | 7 | 122.3 ns | 0.3% | 90.0 ns | 0.3% | 1.36× | 1.35–1.38 | 0.0006 |
| `fstree.encode_blob` | large-8MiB-duplicate | 1 | 7 | 1.516 ms | 0.4% | 1.099 ms | 0.7% | 1.38× | 1.37–1.49 | 0.0006 |
| `fstree.encode_blob` | large-8MiB-random | 1 | 7 | 1.528 ms | 0.9% | 1.122 ms | 0.4% | 1.36× | 1.35–1.54 | 0.0006 |
| `fstree.encode_blob` | large-8MiB-text | 1 | 7 | 1.524 ms | 0.4% | 1.119 ms | 0.7% | 1.36× | 1.36–1.40 | 0.0006 |
| `fstree.encode_blob` | tiny-64B-duplicate | 1 | 7 | 135.3 ns | 0.1% | 107.2 ns | 0.6% | 1.26× | 1.26–1.37 | 0.0006 |
| `fstree.encode_blob` | tiny-64B-random | 1 | 7 | 135.8 ns | 0.5% | 107.7 ns | 0.3% | 1.26× | 1.24–1.27 | 0.0006 |
| `fstree.encode_blob` | tiny-64B-text | 1 | 7 | 135.4 ns | 0.3% | 108.4 ns | 0.5% | 1.25× | 1.24–1.26 | 0.0006 |
| `fstree.encode_dir_leaf` | entries-1024 | 1 | 7 | 152.2 ns | 0.8% | 50.4 ns | 1.0% | 3.02× | 2.98–3.13 | 0.0006 |
| `fstree.encode_dir_leaf` | entries-128 | 1 | 7 | 165.4 ns | 0.8% | 59.4 ns | 0.7% | 2.78× | 2.72–2.84 | 0.0006 |
| `fstree.encode_dir_leaf` | entries-128-partial-change | 1 | 7 | 175.6 ns | 0.2% | 68.8 ns | 1.3% | 2.55× | 2.53–2.64 | 0.0006 |
| `fstree.encode_dir_leaf` | entries-128-with-xattrs | 1 | 7 | 175.3 ns | 0.2% | 69.3 ns | 0.4% | 2.53× | 2.50–2.54 | 0.0006 |
| `fstree.encode_dir_leaf` | entries-8 | 1 | 7 | 235.0 ns | 0.1% | 129.7 ns | 0.6% | 1.81× | 1.79–1.82 | 0.0006 |
| `fstree.encode_dir_node` | pairs-1024 | 1 | 7 | 65.2 ns | 0.5% | 31.6 ns | 0.3% | 2.06× | 2.04–2.08 | 0.0006 |
| `fstree.encode_dir_node` | pairs-128 | 1 | 7 | 70.2 ns | 1.1% | 42.9 ns | 0.1% | 1.64× | 1.60–1.68 | 0.0006 |
| `fstree.encode_dir_node` | pairs-8 | 1 | 7 | 133.8 ns | 2.1% | 90.2 ns | 1.1% | 1.48× | 1.44–1.53 | 0.0006 |
| `fstree.encode_file_node` | children-1024 | 1 | 7 | 39.1 ns | 0.5% | 13.5 ns | 0.8% | 2.89× | 2.86–3.21 | 0.0006 |
| `fstree.encode_file_node` | children-128 | 1 | 7 | 48.1 ns | 0.4% | 18.8 ns | 0.2% | 2.56× | 2.55–2.59 | 0.0021 |
| `fstree.encode_file_node` | children-65536 | 1 | 7 | 42.4 ns | 0.7% | 12.5 ns | 0.1% | 3.41× | 3.35–3.43 | 0.0006 |
| `fstree.encode_file_node` | children-8 | 1 | 7 | 109.4 ns | 4.1% | 45.7 ns | 0.1% | 2.39× | 2.29–2.62 | 0.0021 |
| `fstree.encode_xattr_set` | xattrs-64 | 1 | 7 | 427.3 ns | 0.5% | 97.1 ns | 1.5% | 4.40× | 4.32–4.53 | 0.0006 |
| `fstree.index_builder_file` | children-1024 | 1 | 7 | 130.0 ns | 0.4% | 70.3 ns | 0.1% | 1.85× | 1.83–1.86 | 0.0006 |
| `fstree.index_builder_file` | children-128 | 1 | 7 | 197.0 ns | 3.0% | 66.5 ns | 0.2% | 2.96× | 2.84–3.05 | 0.0021 |
| `fstree.index_builder_file` | children-65536 | 1 | 7 | 118.2 ns | 0.9% | 68.6 ns | 0.2% | 1.72× | 1.70–1.77 | 0.0006 |
| `fstree.index_builder_file` | children-8 | 1 | 7 | 1.220 us | 3.7% | 68.9 ns | 3.6% | 17.71× | 16.29–20.89 | 0.0021 |
| `fstree.list_entries` | widest/first-page-100 | 1 | 7 | 1.380 us | 0.9% | 838.9 ns | 1.3% | 1.64× | 1.59–1.67 | 0.0006 |
| `fstree.list_entries` | width-16/full-paging-100 | 1 | 7 | 557.3 ns | 9.2% | 221.0 ns | 1.4% | 2.52× | 2.27–2.85 | 0.0006 |
| `fstree.list_entries` | width-256/full-paging-100 | 1 | 7 | 657.6 ns | 0.9% | 399.2 ns | 2.8% | 1.65× | 1.62–1.74 | 0.0006 |
| `fstree.list_entries` | width-40000/full-paging-100 | 1 | 7 | 1.357 us | 0.4% | 810.8 ns | 0.5% | 1.67× | 1.61–1.68 | 0.0006 |
| `fstree.list_entries` | width-4096/full-paging-100 | 1 | 7 | 1.151 us | 1.3% | 690.2 ns | 1.0% | 1.67× | 1.61–1.72 | 0.0006 |
| `fstree.lookup_entry` | width-16/hit | 1 | 7 | 5.070 us | 0.8% | 2.969 us | 0.8% | 1.71× | 1.66–1.79 | 0.0006 |
| `fstree.lookup_entry` | width-16/miss | 1 | 7 | 5.393 us | 1.7% | 2.972 us | 1.6% | 1.81× | 1.75–1.88 | 0.0006 |
| `fstree.lookup_entry` | width-256/hit | 1 | 7 | 41.832 us | 0.7% | 24.972 us | 0.6% | 1.68× | 1.61–1.69 | 0.0006 |
| `fstree.lookup_entry` | width-256/miss | 1 | 7 | 639.0 ns | 0.6% | 212.5 ns | 1.0% | 3.01× | 2.95–3.19 | 0.0006 |
| `fstree.lookup_entry` | width-40000/hit | 1 | 7 | 103.122 us | 0.4% | 61.831 us | 0.2% | 1.67× | 1.61–1.69 | 0.0006 |
| `fstree.lookup_entry` | width-40000/miss | 1 | 7 | 639.7 ns | 0.4% | 214.8 ns | 1.3% | 2.98× | 2.94–3.04 | 0.0006 |
| `fstree.lookup_entry` | width-4096/hit | 1 | 7 | 79.979 us | 0.6% | 48.160 us | 0.3% | 1.66× | 1.61–1.68 | 0.0006 |
| `fstree.lookup_entry` | width-4096/miss | 1 | 7 | 3.756 us | 0.8% | 2.142 us | 1.1% | 1.75× | 1.72–1.83 | 0.0006 |
| `fstree.reachable_keys` | deep | auto | 7 | 5.994 us | 3.5% | 33.939 us | 0.8% | 0.18× | 0.15–0.18 | 0.0006 |
| `fstree.reachable_keys` | file-corpus | auto | 7 | 582.2 ns | 3.1% | 2.302 us | 8.1% | 0.25× | 0.21–0.28 | 0.0006 |
| `fstree.reachable_keys` | width-16 | auto | 7 | 1.264 us | 6.4% | 6.292 us | 2.0% | 0.20× | 0.19–0.22 | 0.0006 |
| `fstree.reachable_keys` | width-256 | auto | 7 | 343.5 ns | 0.5% | 699.2 ns | 1.0% | 0.49× | 0.47–0.51 | 0.0006 |
| `fstree.reachable_keys` | width-40000 | auto | 7 | 120.0 ns | 1.0% | 101.6 ns | 0.9% | 1.18× | 1.16–1.20 | 0.0006 |
| `fstree.reachable_keys` | width-4096 | auto | 7 | 143.1 ns | 0.7% | 149.2 ns | 1.2% | 0.96× | 0.94–1.00 | 0.0728 |
| `fstree.resolve_entry` | depth-1 | 1 | 7 | 6.203 us | 1.4% | 3.453 us | 0.4% | 1.80× | 1.75–1.81 | 0.0006 |
| `fstree.resolve_entry` | depth-12 | 1 | 7 | 15.935 us | 0.4% | 8.227 us | 1.3% | 1.94× | 1.91–2.10 | 0.0006 |
| `fstree.resolve_entry` | depth-24 | 1 | 7 | 26.578 us | 0.9% | 13.572 us | 1.3% | 1.96× | 1.89–2.01 | 0.0006 |
| `fstree.resolve_entry` | depth-4 | 1 | 7 | 8.933 us | 1.4% | 4.725 us | 0.7% | 1.89× | 1.78–1.91 | 0.0006 |
| `fstree.resolve_path` | depth-1 | 1 | 7 | 894.0 ns | 1.1% | 470.9 ns | 0.2% | 1.90× | 1.83–1.93 | 0.0006 |
| `fstree.resolve_path` | depth-12 | 1 | 7 | 10.182 us | 1.6% | 5.218 us | 0.4% | 1.95× | 1.93–2.01 | 0.0006 |
| `fstree.resolve_path` | depth-24 | 1 | 7 | 20.444 us | 1.2% | 10.344 us | 0.7% | 1.98× | 1.93–2.01 | 0.0006 |
| `fstree.resolve_path` | depth-4 | 1 | 7 | 3.357 us | 0.5% | 1.767 us | 1.0% | 1.90× | 1.88–1.98 | 0.0006 |
| `fstree.write_content` | file-corpus | 1 | 7 | 165.0 ns | 2.9% | 25.577 us | 23.8% | 0.01× | 0.00–0.01 | 0.0006 |

### `gc`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `gc.close` | idle | 8 | 7 | 331.0 ns | 6.0% | 791.0 ns | 25.4% | 0.42× | 0.31–0.70 | 0.0021 |
| `gc.open` | populated | 8 | 7 | 44.555 us | 9.7% | 31.551 us | 4.9% | 1.41× | 1.13–1.54 | 0.0006 |
| `gc.prepare_ref` | missing-root | 8 | 7 | 2.523 us | 5.9% | 167.0 ns | 7.8% | 15.10× | 14.19–18.44 | 0.0006 |
| `gc.prepare_ref` | tree-root/abort | 8 | 7 | 2.891 ms | 2.4% | 1.690 ms | 3.6% | 1.71× | 1.64–1.82 | 0.0006 |
| `gc.prepare_ref` | tree-root/commit | 8 | 7 | 2.833 ms | 1.2% | 1.713 ms | 1.8% | 1.65× | 1.62–1.85 | 0.0006 |
| `gc.release_ref` | batch | 8 | 7 | 6.5 ns | 0.4% | 0.8 ns | 1.7% | 7.81× | 5.26–7.90 | 0.0021 |
| `gc.run` | nothing-to-reclaim | 8 | 7 | 3.763 ms | 2.4% | 1.617 ms | 2.3% | 2.33× | 1.72–2.40 | 0.0006 |
| `gc.run` | reclaimable-packs | 8 | 7 | 14.911 ms | 4.5% | 11.478 ms | 3.9% | 1.30× | 1.07–1.40 | 0.0023 |
| `gc.status` | mark+score | 8 | 7 | 5.281 ms | 2.8% | 3.799 ms | 6.8% | 1.39× | 1.28–1.54 | 0.0006 |
| `gc.why` | live-root | 8 | 7 | 55.416 us | 8.4% | 5.731 us | 3.7% | 9.67× | 8.71–11.79 | 0.0021 |
| `gc.wipe` | store-reset | 8 | 7 | 6.599 ms | 8.8% | 4.931 ms | 0.6% | 1.34× | 1.06–1.45 | 0.0006 |

### `inbox`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `inbox.close` | drained | 8 | 7 | 6.419 ms | 4.1% | 6.166 ms | 7.5% | 1.04× | 0.44–1.28 | 0.7104 |
| `inbox.commit` | packs/duplicate | 8 | 7 | 19.739 us | 7.1% | 20.646 us | 1.5% | 0.96× | 0.85–1.13 | 0.1649 |
| `inbox.discard` | packs | 8 | 7 | 17.391 us | 7.4% | 17.894 us | 1.5% | 0.97× | 0.87–1.13 | 0.3176 |
| `inbox.drain` | packs/new | 8 | 7 | 294.966 us | 1.8% | 236.930 us | 2.0% | 1.24× | 1.20–1.29 | 0.0006 |
| `inbox.open` | empty | 8 | 7 | 65.946 us | 5.8% | 103.979 us | 3.6% | 0.63× | 0.54–0.68 | 0.0006 |
| `inbox.open` | sweeps-staged-tmp-files | 8 | 7 | 18.066 us | 3.0% | 24.607 us | 9.7% | 0.73× | 0.65–0.82 | 0.0023 |
| `inbox.stage` | packs | 8 | 7 | 178.145 us | 1.4% | 181.962 us | 1.6% | 0.98× | 0.95–1.02 | 0.3176 |

### `ingest`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `ingest.dir` | single-file/jobs-1 | 1 | 7 | 27.806 ms | 0.6% | 15.930 ms | 2.4% | 1.75× | 1.69–1.83 | 0.0006 |
| `ingest.dir` | tree/fresh-store/jobs-1 | 1 | 7 | 115.712 us | 1.6% | 20.505 us | 0.6% | 5.64× | 5.53–5.85 | 0.0006 |
| `ingest.dir` | tree/fresh-store/jobs-N | 8 | 7 | 91.535 us | 1.5% | 10.463 us | 5.5% | 8.75× | 8.25–9.43 | 0.0006 |
| `ingest.dir` | tree/incremental-change/jobs-1 | 1 | 7 | 93.227 us | 0.8% | 9.885 us | 0.6% | 9.43× | 9.21–9.53 | 0.0006 |
| `ingest.dir` | tree/incremental-change/jobs-N | 8 | 7 | 88.876 us | 0.4% | 7.443 us | 1.4% | 11.94× | 11.51–12.12 | 0.0006 |
| `ingest.dir` | tree/unchanged-repeat/jobs-1 | 1 | 7 | 93.500 us | 0.5% | 9.760 us | 0.4% | 9.58× | 9.51–9.66 | 0.0006 |
| `ingest.dir` | tree/unchanged-repeat/jobs-N | 8 | 7 | 87.694 us | 1.0% | 7.694 us | 2.3% | 11.40× | 11.05–11.76 | 0.0006 |
| `ingest.objects` | tree/jobs-1 | 1 | 7 | 93.455 us | 1.0% | 9.763 us | 0.5% | 9.57× | 9.30–9.67 | 0.0006 |
| `ingest.objects` | tree/jobs-N | 8 | 7 | 87.217 us | 1.5% | 7.152 us | 1.2% | 12.19× | 11.79–12.40 | 0.0006 |
| `ingest.scan` | filtered/jobs-1 | 1 | 7 | 2.240 us | 1.5% | 2.645 us | 1.4% | 0.85× | 0.80–0.87 | 0.0006 |
| `ingest.scan` | filtered/jobs-N | 8 | 7 | 974.0 ns | 1.0% | 5.310 us | 8.5% | 0.18× | 0.18–0.21 | 0.0006 |
| `ingest.scan` | unfiltered/jobs-1 | 1 | 7 | 1.311 us | 0.1% | 1.570 us | 1.0% | 0.83× | 0.83–0.84 | 0.0006 |

### `key`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `key.accessors` | type+length+length_size+hash | 1 | 7 | 12.5 ns | 0.1% | 8.3 ns | 0.0% | 1.51× | 1.51–1.52 | 0.0019 |
| `key.new` | 1MiB-duplicate | 1 | 7 | 190.315 us | 0.3% | 132.571 us | 8.7% | 1.44× | 1.29–1.58 | 0.0006 |
| `key.new` | 1MiB-random | 1 | 7 | 192.488 us | 0.7% | 120.894 us | 0.3% | 1.59× | 1.44–1.85 | 0.0006 |
| `key.new` | 1MiB-text | 1 | 7 | 192.216 us | 0.4% | 122.753 us | 0.7% | 1.57× | 1.55–1.60 | 0.0006 |
| `key.new` | 4KiB-duplicate | 1 | 7 | 1.762 us | 0.1% | 1.070 us | 0.1% | 1.65× | 1.64–1.68 | 0.0006 |
| `key.new` | 4KiB-random | 1 | 7 | 1.785 us | 0.4% | 1.070 us | 0.1% | 1.67× | 1.66–1.70 | 0.0006 |
| `key.new` | 4KiB-text | 1 | 7 | 1.799 us | 0.3% | 1.073 us | 0.3% | 1.68× | 1.67–1.75 | 0.0006 |
| `key.new` | empty | 1 | 7 | 107.0 ns | 0.6% | 77.5 ns | 1.5% | 1.38× | 1.25–1.40 | 0.0006 |
| `key.new` | large-8MiB-duplicate | 1 | 7 | 1.521 ms | 0.3% | 949.919 us | 0.3% | 1.60× | 1.59–1.61 | 0.0021 |
| `key.new` | large-8MiB-random | 1 | 7 | 1.542 ms | 1.5% | 952.039 us | 0.1% | 1.62× | 1.60–1.76 | 0.0006 |
| `key.new` | large-8MiB-text | 1 | 7 | 1.528 ms | 0.3% | 953.602 us | 0.1% | 1.60× | 1.60–1.61 | 0.0006 |
| `key.new` | tiny-64B-duplicate | 1 | 7 | 107.1 ns | 0.2% | 82.4 ns | 0.3% | 1.30× | 1.28–1.31 | 0.0006 |
| `key.new` | tiny-64B-random | 1 | 7 | 107.2 ns | 0.2% | 81.9 ns | 0.9% | 1.31× | 1.29–1.32 | 0.0006 |
| `key.new` | tiny-64B-text | 1 | 7 | 106.8 ns | 0.2% | 81.6 ns | 0.3% | 1.31× | 1.29–1.32 | 0.0006 |
| `key.new_from_hash` | batch | 1 | 7 | 41.2 ns | 1.0% | 35.3 ns | 0.0% | 1.17× | 1.15–1.22 | 0.0021 |
| `key.parse` | canonical | 1 | 7 | 32.0 ns | 0.4% | 24.9 ns | 0.0% | 1.28× | 1.28–1.29 | 0.0019 |
| `key.parse` | malformed | 1 | 7 | 57.3 ns | 0.9% | 0.8 ns | 0.1% | 69.72× | 67.66–70.44 | 0.0021 |
| `key.string` | hex | 1 | 7 | 87.8 ns | 0.7% | 514.3 ns | 0.5% | 0.17× | 0.17–0.18 | 0.0006 |
| `key.type_string` | names | 1 | 7 | 12.2 ns | 0.3% | 14.4 ns | 0.8% | 0.85× | 0.84–0.85 | 0.0006 |
| `key.validate` | canonical | 1 | 7 | 6.6 ns | 0.2% | 0.9 ns | 0.1% | 7.49× | 7.34–7.50 | 0.0020 |

### `packstore`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `packstore.append_record` | pre-encoded+sync | 1 | 7 | 6.664 us | 2.4% | 7.514 us | 1.5% | 0.89× | 0.84–0.93 | 0.0006 |
| `packstore.barrier` | begin+observe+abort | 1 | 7 | 53.6 ns | 0.9% | 49.3 ns | 0.8% | 1.09× | 1.08–1.13 | 0.0006 |
| `packstore.close` | populated | 1 | 7 | 1.105 ms | 2.2% | 943.623 us | 2.3% | 1.17× | 0.51–1.21 | 0.2086 |
| `packstore.compact` | 90-percent-dead | 1 | 7 | 469.1 ns | 1.7% | 377.8 ns | 2.8% | 1.24× | 1.18–1.31 | 0.0012 |
| `packstore.compact` | nothing-dead | 1 | 7 | 75.0 ns | 2.3% | 39.3 ns | 1.3% | 1.91× | 1.85–1.98 | 0.0006 |
| `packstore.get` | hit | 1 | 7 | 7.109 us | 1.4% | 4.462 us | 2.1% | 1.59× | 1.50–1.66 | 0.0006 |
| `packstore.get` | miss | 1 | 7 | 54.0 ns | 0.2% | 51.2 ns | 4.0% | 1.06× | 0.98–1.10 | 0.2086 |
| `packstore.get_record` | hit | 1 | 7 | 366.3 ns | 1.1% | 217.1 ns | 2.8% | 1.69× | 1.57–1.72 | 0.0006 |
| `packstore.has` | hit | 1 | 7 | 92.8 ns | 1.2% | 59.6 ns | 7.0% | 1.56× | 1.23–1.75 | 0.0006 |
| `packstore.has` | miss | 1 | 7 | 50.0 ns | 0.6% | 37.2 ns | 1.2% | 1.34× | 1.31–1.39 | 0.0006 |
| `packstore.has_outside` | sealed-segment | 1 | 7 | 82.6 ns | 1.5% | 40.5 ns | 3.4% | 2.04× | 1.88–2.12 | 0.0006 |
| `packstore.liveness` | tenth-live | 1 | 7 | 727.189 us | 2.7% | 595.438 us | 0.8% | 1.22× | 1.18–1.27 | 0.0006 |
| `packstore.mark_set_contains` | all-keys | 1 | 7 | 128.6 ns | 0.5% | 125.1 ns | 1.0% | 1.03× | 1.02–1.12 | 0.0012 |
| `packstore.mark_set_mark` | all-keys | 1 | 7 | 70.5 ns | 0.6% | 63.6 ns | 0.5% | 1.11× | 1.10–1.23 | 0.0006 |
| `packstore.missing` | half-present | 1 | 7 | 84.9 ns | 2.2% | 122.5 ns | 2.6% | 0.69× | 0.66–0.72 | 0.0006 |
| `packstore.new_mark_set` | snapshot | 1 | 7 | 138.915 us | 1.8% | 59.750 us | 0.3% | 2.32× | 2.30–2.60 | 0.0006 |
| `packstore.oldest_inflight_write` | idle | 1 | 7 | 13.4 ns | 0.9% | 3.2 ns | 0.2% | 4.18× | 4.14–4.22 | 0.0006 |
| `packstore.open` | empty | 1 | 7 | 26.470 us | 5.1% | 10.089 us | 0.6% | 2.62× | 1.99–2.74 | 0.0006 |
| `packstore.open` | populated-reopen | 1 | 7 | 1.132 ms | 2.4% | 1.406 ms | 1.7% | 0.80× | 0.78–0.93 | 0.0006 |
| `packstore.put` | 1MiB-duplicate/sync-off | 1 | 7 | 513.641 us | 1.8% | 271.078 us | 1.9% | 1.89× | 1.84–2.00 | 0.0006 |
| `packstore.put` | 1MiB-random/sync-off | 1 | 7 | 975.724 us | 1.0% | 522.518 us | 1.8% | 1.87× | 1.84–2.00 | 0.0021 |
| `packstore.put` | 1MiB-text/sync-off | 1 | 7 | 3.734 ms | 1.3% | 1.973 ms | 1.2% | 1.89× | 1.71–1.95 | 0.0006 |
| `packstore.put` | 4KiB-duplicate/sync-off | 1 | 7 | 692.9 ns | 5.8% | 550.0 ns | 3.2% | 1.26× | 1.13–1.36 | 0.0012 |
| `packstore.put` | 4KiB-random/already-present | 1 | 7 | 93.7 ns | 2.0% | 75.2 ns | 0.6% | 1.25× | 1.22–1.28 | 0.0006 |
| `packstore.put` | 4KiB-random/sync-off | 1 | 7 | 7.185 us | 2.9% | 7.573 us | 0.4% | 0.95× | 0.93–1.00 | 0.0379 |
| `packstore.put` | 4KiB-random/sync-on | 1 | 7 | 91.743 us | 0.3% | 91.471 us | 0.3% | 1.00× | 1.00–1.01 | 0.3829 |
| `packstore.put` | 4KiB-text/sync-off | 1 | 7 | 26.925 us | 2.3% | 16.155 us | 0.4% | 1.67× | 1.62–1.78 | 0.0006 |
| `packstore.put` | empty/sync-off | 1 | 7 | 100.8 ns | 0.7% | 87.8 ns | 0.9% | 1.15× | 1.14–1.17 | 0.0006 |
| `packstore.put` | large-8MiB-duplicate/sync-off | 1 | 7 | 3.280 ms | 1.4% | 1.576 ms | 2.2% | 2.08× | 1.95–2.19 | 0.0006 |
| `packstore.put` | large-8MiB-random/sync-off | 1 | 7 | 6.678 ms | 1.4% | 3.099 ms | 1.6% | 2.15× | 2.08–2.20 | 0.0006 |
| `packstore.put` | large-8MiB-text/sync-off | 1 | 7 | 26.765 ms | 0.6% | 14.494 ms | 0.5% | 1.85× | 1.79–1.92 | 0.0006 |
| `packstore.put` | tiny-64B-duplicate/sync-off | 1 | 7 | 92.0 ns | 0.9% | 82.9 ns | 0.9% | 1.11× | 1.07–1.13 | 0.0262 |
| `packstore.put` | tiny-64B-random/sync-off | 1 | 7 | 1.703 us | 0.8% | 1.653 us | 0.2% | 1.03× | 1.02–1.05 | 0.0006 |
| `packstore.put` | tiny-64B-text/sync-off | 1 | 7 | 2.317 us | 0.4% | 2.214 us | 0.0% | 1.05× | 1.04–1.07 | 0.0006 |
| `packstore.put_verified` | already-intact | 1 | 7 | 87.899 us | 1.2% | 53.310 us | 0.5% | 1.65× | 1.61–1.71 | 0.0006 |
| `packstore.record` | by-location | 1 | 7 | 461.5 ns | 19.4% | 594.9 ns | 6.0% | 0.78× | 0.59–1.33 | 0.2086 |
| `packstore.remove` | one-sealed-segment | 1 | 7 | 819.035 us | 10.6% | 627.218 us | 1.1% | 1.31× | 1.15–1.51 | 0.0023 |
| `packstore.scan_index` | all-segments | 1 | 7 | 29.3 ns | 1.1% | 23.9 ns | 0.4% | 1.22× | 1.20–1.23 | 0.0006 |
| `packstore.segments` | list | 1 | 7 | 7.124 us | 0.3% | 6.099 us | 0.4% | 1.17× | 1.15–1.18 | 0.0006 |
| `packstore.sort_by_location` | scattered | 1 | 7 | 190.4 ns | 3.9% | 90.8 ns | 2.3% | 2.10× | 1.87–2.23 | 0.0006 |
| `packstore.stored_size` | hit | 1 | 7 | 95.7 ns | 2.0% | 53.4 ns | 10.3% | 1.79× | 1.43–2.02 | 0.0006 |
| `packstore.verify` | full-scrub | 1 | 7 | 9.258 us | 0.6% | 5.898 us | 0.3% | 1.57× | 1.54–1.59 | 0.0006 |
| `packstore.wipe` | populated | 1 | 7 | 5.210 ms | 16.0% | 3.837 ms | 1.7% | 1.36× | 1.08–1.51 | 0.0006 |
| `packstore.write_batch` | mixed-objects | 1 | 7 | 32.066 us | 0.8% | 21.106 us | 0.3% | 1.52× | 1.49–1.53 | 0.0006 |
| `packstore.write_parallel` | duplicate-stream/writers-N | 8 | 7 | 327.8 ns | 3.3% | 1.189 us | 0.4% | 0.28× | 0.27–0.29 | 0.0006 |
| `packstore.write_parallel` | mixed-objects/writers-1/verify-false | 1 | 7 | 32.445 us | 0.2% | 21.616 us | 0.4% | 1.50× | 1.49–1.52 | 0.0006 |
| `packstore.write_parallel` | mixed-objects/writers-1/verify-true | 1 | 7 | 35.095 us | 0.3% | 23.242 us | 0.5% | 1.51× | 1.50–1.53 | 0.0006 |
| `packstore.write_parallel` | mixed-objects/writers-N/verify-false | 8 | 7 | 9.125 us | 1.0% | 5.456 us | 0.3% | 1.67× | 1.58–1.70 | 0.0006 |
| `packstore.write_parallel` | mixed-objects/writers-N/verify-true | 8 | 7 | 9.517 us | 2.1% | 5.515 us | 0.7% | 1.73× | 1.62–1.80 | 0.0006 |

### `reference`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `reference.decode` | signed | 1 | 7 | 863.0 ns | 1.6% | 430.9 ns | 4.1% | 2.00× | 1.88–2.12 | 0.0006 |
| `reference.encode` | signed | 1 | 7 | 277.8 ns | 5.3% | 167.9 ns | 1.1% | 1.65× | 1.56–1.79 | 0.0006 |
| `reference.signature_payload` | signed | 1 | 7 | 267.5 ns | 7.5% | 171.7 ns | 0.9% | 1.56× | 1.51–1.70 | 0.0006 |
| `reference.validate_name` | valid | 1 | 7 | 57.0 ns | 1.6% | 29.5 ns | 0.4% | 1.93× | 1.91–1.99 | 0.0006 |
| `reference.validate_user` | valid | 1 | 7 | 75.1 ns | 6.9% | 90.1 ns | 2.6% | 0.83× | 0.68–0.93 | 0.0070 |

### `refstore`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `refstore.all` | records | 1 | 7 | 108.5 ns | 4.0% | 161.5 ns | 2.3% | 0.67× | 0.65–0.80 | 0.0006 |
| `refstore.close` | populated | 1 | 7 | 133.355 us | 1.8% | 920.308 us | 0.7% | 0.14× | 0.14–0.16 | 0.0006 |
| `refstore.delete` | records | 1 | 7 | 1.688 us | 1.5% | 39.647 us | 0.4% | 0.04× | 0.04–0.05 | 0.0006 |
| `refstore.get` | hit | 1 | 7 | 1.026 us | 1.0% | 559.4 ns | 0.3% | 1.83× | 1.80–1.87 | 0.0006 |
| `refstore.get` | miss | 1 | 7 | 210.0 ns | 2.4% | 345.3 ns | 0.7% | 0.61× | 0.59–0.69 | 0.0006 |
| `refstore.open` | empty | 1 | 7 | 748.920 us | 1.7% | 1.271 ms | 0.5% | 0.59× | 0.58–0.61 | 0.0006 |
| `refstore.open` | populated-reopen | 1 | 7 | 2.456 ms | 1.9% | 903.826 us | 0.8% | 2.72× | 2.63–2.78 | 0.0006 |
| `refstore.put` | records/sync-false | 1 | 7 | 476.6 ns | 4.9% | 43.840 us | 0.2% | 0.01× | 0.01–0.01 | 0.0006 |
| `refstore.put` | records/sync-true | 1 | 7 | 60.238 us | 0.3% | 43.995 us | 0.2% | 1.37× | 1.35–1.38 | 0.0006 |
| `refstore.put_batch` | records/sync-false | 1 | 7 | 115.6 ns | 2.0% | 904.3 ns | 0.2% | 0.13× | 0.12–0.13 | 0.0006 |
| `refstore.wipe` | records | 1 | 7 | 1.169 ms | 3.1% | 16.437 ms | 2.7% | 0.07× | 0.07–0.08 | 0.0006 |

### `tar`

| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | Go/Rust | 95 % CI | p |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|
| `tarexport.write` | fixture-tree | 1 | 7 | 2.357 us | 0.1% | 2.267 us | 0.4% | 1.04× | 1.03–1.05 | 0.0006 |
| `tarextract.extract` | fixture-tree | 1 | 7 | 39.363 us | 0.6% | 32.264 us | 0.3% | 1.22× | 1.21–1.37 | 0.0006 |

## Rates

Each operation in the unit it is actually in. `ops/s` counts the core operations
the case declares; `entries/s` and `objects/s` count the tree entries and stored
objects its dimensions record, both computed outside the measured interval. The
byte column names what its bytes are — a metadata walk that never opens a file
body is measured in *logical bytes scanned*, not in bandwidth.

| Operation | Workload | Bytes counted | Go | Rust | Go ops/s | Rust ops/s | Go entries/s | Rust entries/s |
|---|---|---|---:|---:|---:|---:|---:|---:|
| `amberignore.descend` | 8-subdirs | — | — | — | 196.42 k/s | 316.87 k/s | — | — |
| `amberignore.ignored` | mixed-names | — | — | — | 4.74 M/s | 8.44 M/s | — | — |
| `amberignore.ignored` | nil-matcher | — | — | — | 196.92 M/s | 11.38 G/s | — | — |
| `amberignore.root` | load-root | — | — | — | 136.66 k/s | 233.54 k/s | — | — |
| `amberpack.decode_payload` | mixed-records/producer-go | encoded bytes | 1239.7 MiB/s | 1798.6 MiB/s | 64.54 k/s | 93.64 k/s | — | — |
| `amberpack.decode_payload` | mixed-records/producer-rust | encoded bytes | 1320.5 MiB/s | 1962.8 MiB/s | 68.86 k/s | 102.36 k/s | — | — |
| `amberpack.encode_record` | 1MiB-duplicate | payload bytes | 2275.2 MiB/s | 3850.6 MiB/s | 2.28 k/s | 3.85 k/s | — | — |
| `amberpack.encode_record` | 1MiB-random | payload bytes | 2601.8 MiB/s | 3873.7 MiB/s | 2.60 k/s | 3.87 k/s | — | — |
| `amberpack.encode_record` | 1MiB-text | payload bytes | 314.9 MiB/s | 562.4 MiB/s | 315 /s | 562 /s | — | — |
| `amberpack.encode_record` | 4KiB-duplicate | payload bytes | 1118.6 MiB/s | 887.9 MiB/s | 286.35 k/s | 227.31 k/s | — | — |
| `amberpack.encode_record` | 4KiB-random | payload bytes | 1064.9 MiB/s | 868.0 MiB/s | 272.60 k/s | 222.20 k/s | — | — |
| `amberpack.encode_record` | 4KiB-text | payload bytes | 160.8 MiB/s | 274.8 MiB/s | 41.17 k/s | 70.34 k/s | — | — |
| `amberpack.encode_record` | empty | payload bytes | — | — | 16.13 M/s | 3.45 M/s | — | — |
| `amberpack.encode_record` | large-8MiB-duplicate | payload bytes | 2099.2 MiB/s | 3861.1 MiB/s | 262 /s | 483 /s | — | — |
| `amberpack.encode_record` | large-8MiB-random | payload bytes | 2181.3 MiB/s | 3851.2 MiB/s | 273 /s | 481 /s | — | — |
| `amberpack.encode_record` | large-8MiB-text | payload bytes | 311.5 MiB/s | 567.9 MiB/s | 39 /s | 71 /s | — | — |
| `amberpack.encode_record` | tiny-64B-duplicate | payload bytes | 118.9 MiB/s | 98.6 MiB/s | 1.95 M/s | 1.62 M/s | — | — |
| `amberpack.encode_record` | tiny-64B-random | payload bytes | 108.6 MiB/s | 98.1 MiB/s | 1.78 M/s | 1.61 M/s | — | — |
| `amberpack.encode_record` | tiny-64B-text | payload bytes | 53.8 MiB/s | 52.6 MiB/s | 881.01 k/s | 861.40 k/s | — | — |
| `amberpack.parse_record` | corrupt-crc/producer-go | — | — | — | 7.60 M/s | 9.80 M/s | — | — |
| `amberpack.parse_record` | corrupt-crc/producer-rust | — | — | — | 7.74 M/s | 9.82 M/s | — | — |
| `amberpack.parse_record` | mixed-records/producer-go | encoded bytes | 26535.7 MiB/s | 8442.9 MiB/s | 1.38 M/s | 439.56 k/s | — | — |
| `amberpack.parse_record` | mixed-records/producer-rust | encoded bytes | 26847.2 MiB/s | 8450.3 MiB/s | 1.40 M/s | 440.67 k/s | — | — |
| `amberpack.reader_all` | mixed-objects/producer-go | encoded bytes | 1165.6 MiB/s | 1414.3 MiB/s | 60.68 k/s | 73.63 k/s | — | — |
| `amberpack.reader_all` | mixed-objects/producer-rust | encoded bytes | 1194.4 MiB/s | 1509.4 MiB/s | 62.29 k/s | 78.71 k/s | — | — |
| `amberpack.reader_all` | truncated-stream/producer-go | — | — | — | 190 /s | 305 /s | — | — |
| `amberpack.reader_all` | truncated-stream/producer-rust | — | — | — | 197 /s | 325 /s | — | — |
| `amberpack.reader_records` | mixed-objects/producer-go | encoded bytes | 13045.7 MiB/s | 6790.7 MiB/s | 679.19 k/s | 353.54 k/s | — | — |
| `amberpack.reader_records` | mixed-objects/producer-rust | encoded bytes | 13338.3 MiB/s | 6812.0 MiB/s | 695.57 k/s | 355.23 k/s | — | — |
| `amberpack.writer_add` | mixed-objects | payload bytes | 482.6 MiB/s | 816.1 MiB/s | 14.92 k/s | 25.23 k/s | — | — |
| `amberpack.writer_add_record` | pre-encoded-records/producer-go | encoded bytes | 660147.7 MiB/s | 2529320.1 MiB/s | 34.37 M/s | 131.68 M/s | — | — |
| `amberpack.writer_add_record` | pre-encoded-records/producer-rust | encoded bytes | 651005.3 MiB/s | 2485227.1 MiB/s | 33.95 M/s | 129.60 M/s | — | — |
| `cbor.decode_xattrs` | xattrs-3 | encoded bytes | 165.2 MiB/s | 390.7 MiB/s | 2.44 M/s | 5.77 M/s | — | — |
| `cbor.decode_xattrs` | xattrs-64 | encoded bytes | 472.6 MiB/s | 757.1 MiB/s | 112.17 k/s | 179.70 k/s | — | — |
| `cbor.encode_xattrs` | xattrs-3 | encoded bytes | 152.4 MiB/s | 268.2 MiB/s | 2.25 M/s | 3.96 M/s | — | — |
| `cbor.encode_xattrs` | xattrs-64 | encoded bytes | 167.4 MiB/s | 933.5 MiB/s | 39.73 k/s | 221.56 k/s | — | — |
| `chunkers.item_chunker` | is_boundary/bits-7 | — | — | — | 18.97 M/s | 30.73 M/s | — | — |
| `chunkers.new_item_chunker` | bits-4..12 | — | — | — | 76.37 M/s | 77.17 M/s | — | — |
| `chunkers.split_bytes` | compressible/4-16-64KiB | payload bytes | 1110.0 MiB/s | 1551.9 MiB/s | 17.76 k/s | 24.83 k/s | — | — |
| `chunkers.split_bytes` | compressible/default-sizes | payload bytes | 1114.8 MiB/s | 1514.0 MiB/s | 1.11 k/s | 1.51 k/s | — | — |
| `chunkers.split_bytes` | random/default-sizes | payload bytes | 1476.1 MiB/s | 1631.4 MiB/s | 20.32 k/s | 22.46 k/s | — | — |
| `chunkers.split_bytes` | tiny-64KiB/default-sizes | payload bytes | 1252.4 MiB/s | 2823.8 MiB/s | 20.04 k/s | 45.18 k/s | — | — |
| `fstree.check_complete` | incomplete/jobs-1 | — | — | — | 30.85 k/s | 166.64 k/s | — | — |
| `fstree.check_complete` | widest/jobs-1 | — | — | — | 1.50 M/s | 3.67 M/s | — | — |
| `fstree.check_complete` | widest/jobs-N | — | — | — | 2.85 M/s | 7.74 M/s | — | — |
| `fstree.check_complete` | width-16/jobs-1 | — | — | — | 646.14 k/s | 3.19 M/s | — | — |
| `fstree.check_complete` | width-256/jobs-1 | — | — | — | 1.33 M/s | 3.73 M/s | — | — |
| `fstree.check_complete` | width-40000/jobs-1 | — | — | — | 1.49 M/s | 3.66 M/s | — | — |
| `fstree.check_complete` | width-4096/jobs-1 | — | — | — | 1.51 M/s | 3.62 M/s | — | — |
| `fstree.child_keys` | dir-leaf-128 | — | — | — | 21.43 k/s | 34.42 k/s | — | — |
| `fstree.child_keys` | dir-node-128 | — | — | — | 47.18 k/s | 70.52 k/s | — | — |
| `fstree.child_keys` | file-node-1024 | — | — | — | 11.36 k/s | 13.99 k/s | — | — |
| `fstree.collect_entries` | width-16 | — | — | — | 1.48 M/s | 4.13 M/s | 1.48 M/s | 4.13 M/s |
| `fstree.collect_entries` | width-256 | — | — | — | 2.64 M/s | 4.55 M/s | 2.64 M/s | 4.55 M/s |
| `fstree.collect_entries` | width-40000 | — | — | — | 2.52 M/s | 4.52 M/s | 2.52 M/s | 4.52 M/s |
| `fstree.collect_entries` | width-4096 | — | — | — | 2.74 M/s | 4.47 M/s | 2.74 M/s | 4.47 M/s |
| `fstree.decode_dir_leaf` | entries-1024 | encoded bytes | 201.2 MiB/s | 327.6 MiB/s | 2.85 M/s | 4.64 M/s | 2.85 M/s | 4.64 M/s |
| `fstree.decode_dir_leaf` | entries-128 | encoded bytes | 198.2 MiB/s | 331.8 MiB/s | 2.81 M/s | 4.70 M/s | 2.81 M/s | 4.70 M/s |
| `fstree.decode_dir_leaf` | entries-128-partial-change | encoded bytes | 213.4 MiB/s | 354.9 MiB/s | 2.70 M/s | 4.48 M/s | 2.70 M/s | 4.48 M/s |
| `fstree.decode_dir_leaf` | entries-128-with-xattrs | encoded bytes | 213.9 MiB/s | 354.0 MiB/s | 2.70 M/s | 4.47 M/s | 2.70 M/s | 4.47 M/s |
| `fstree.decode_dir_leaf` | entries-8 | encoded bytes | 188.8 MiB/s | 328.2 MiB/s | 2.67 M/s | 4.64 M/s | 2.67 M/s | 4.64 M/s |
| `fstree.decode_dir_node` | pairs-1024 | encoded bytes | 281.6 MiB/s | 479.1 MiB/s | 5.91 M/s | 10.05 M/s | 5.91 M/s | 10.05 M/s |
| `fstree.decode_dir_node` | pairs-128 | encoded bytes | 284.8 MiB/s | 495.3 MiB/s | 5.97 M/s | 10.38 M/s | 5.97 M/s | 10.38 M/s |
| `fstree.decode_dir_node` | pairs-8 | encoded bytes | 254.8 MiB/s | 556.6 MiB/s | 5.33 M/s | 11.64 M/s | 5.33 M/s | 11.64 M/s |
| `fstree.decode_file_node` | children-1024 | encoded bytes | 377.0 MiB/s | 469.8 MiB/s | 11.62 M/s | 14.49 M/s | 11.62 M/s | 14.49 M/s |
| `fstree.decode_file_node` | children-128 | encoded bytes | 371.1 MiB/s | 515.0 MiB/s | 11.44 M/s | 15.88 M/s | 11.44 M/s | 15.88 M/s |
| `fstree.decode_file_node` | children-65536 | encoded bytes | 364.1 MiB/s | 464.4 MiB/s | 11.23 M/s | 14.32 M/s | 11.23 M/s | 14.32 M/s |
| `fstree.decode_file_node` | children-8 | encoded bytes | 294.4 MiB/s | 515.7 MiB/s | 9.05 M/s | 15.85 M/s | 9.05 M/s | 15.85 M/s |
| `fstree.dir_builder` | entries-16 | — | — | — | 733.54 k/s | 3.00 M/s | 733.54 k/s | 3.00 M/s |
| `fstree.dir_builder` | entries-256 | — | — | — | 1.83 M/s | 2.99 M/s | 1.83 M/s | 2.99 M/s |
| `fstree.dir_builder` | entries-40000 | — | — | — | 2.01 M/s | 3.00 M/s | 2.01 M/s | 3.00 M/s |
| `fstree.dir_builder` | entries-4096 | — | — | — | 2.07 M/s | 3.12 M/s | 2.07 M/s | 3.12 M/s |
| `fstree.encode_blob` | 1MiB-duplicate | payload bytes | 5188.0 MiB/s | 7405.9 MiB/s | 5.19 k/s | 7.41 k/s | — | — |
| `fstree.encode_blob` | 1MiB-random | payload bytes | 4939.0 MiB/s | 7457.1 MiB/s | 4.94 k/s | 7.46 k/s | — | — |
| `fstree.encode_blob` | 1MiB-text | payload bytes | 4868.6 MiB/s | 7393.7 MiB/s | 4.87 k/s | 7.39 k/s | — | — |
| `fstree.encode_blob` | 4KiB-duplicate | payload bytes | 2187.0 MiB/s | 3482.3 MiB/s | 559.86 k/s | 891.47 k/s | — | — |
| `fstree.encode_blob` | 4KiB-random | payload bytes | 2149.4 MiB/s | 3484.3 MiB/s | 550.26 k/s | 891.98 k/s | — | — |
| `fstree.encode_blob` | 4KiB-text | payload bytes | 2038.9 MiB/s | 3465.5 MiB/s | 521.95 k/s | 887.16 k/s | — | — |
| `fstree.encode_blob` | empty | payload bytes | — | — | 8.18 M/s | 11.12 M/s | — | — |
| `fstree.encode_blob` | large-8MiB-duplicate | payload bytes | 5278.3 MiB/s | 7277.5 MiB/s | 660 /s | 910 /s | — | — |
| `fstree.encode_blob` | large-8MiB-random | payload bytes | 5235.3 MiB/s | 7132.6 MiB/s | 654 /s | 892 /s | — | — |
| `fstree.encode_blob` | large-8MiB-text | payload bytes | 5250.3 MiB/s | 7151.4 MiB/s | 656 /s | 894 /s | — | — |
| `fstree.encode_blob` | tiny-64B-duplicate | payload bytes | 451.0 MiB/s | 569.6 MiB/s | 7.39 M/s | 9.33 M/s | — | — |
| `fstree.encode_blob` | tiny-64B-random | payload bytes | 449.5 MiB/s | 566.5 MiB/s | 7.36 M/s | 9.28 M/s | — | — |
| `fstree.encode_blob` | tiny-64B-text | payload bytes | 450.7 MiB/s | 562.9 MiB/s | 7.38 M/s | 9.22 M/s | — | — |
| `fstree.encode_dir_leaf` | entries-1024 | encoded bytes | 463.6 MiB/s | 1399.4 MiB/s | 6.57 M/s | 19.83 M/s | 6.57 M/s | 19.83 M/s |
| `fstree.encode_dir_leaf` | entries-128 | encoded bytes | 426.7 MiB/s | 1188.2 MiB/s | 6.04 M/s | 16.83 M/s | 6.04 M/s | 16.83 M/s |
| `fstree.encode_dir_leaf` | entries-128-partial-change | encoded bytes | 450.9 MiB/s | 1150.6 MiB/s | 5.69 M/s | 14.53 M/s | 5.69 M/s | 14.53 M/s |
| `fstree.encode_dir_leaf` | entries-128-with-xattrs | encoded bytes | 451.7 MiB/s | 1141.8 MiB/s | 5.70 M/s | 14.42 M/s | 5.70 M/s | 14.42 M/s |
| `fstree.encode_dir_leaf` | entries-8 | encoded bytes | 300.8 MiB/s | 545.0 MiB/s | 4.25 M/s | 7.71 M/s | 4.25 M/s | 7.71 M/s |
| `fstree.encode_dir_node` | pairs-1024 | encoded bytes | 731.7 MiB/s | 1506.9 MiB/s | 15.34 M/s | 31.60 M/s | 15.34 M/s | 31.60 M/s |
| `fstree.encode_dir_node` | pairs-128 | encoded bytes | 679.4 MiB/s | 1112.4 MiB/s | 14.24 M/s | 23.32 M/s | 14.24 M/s | 23.32 M/s |
| `fstree.encode_dir_node` | pairs-8 | encoded bytes | 357.3 MiB/s | 529.9 MiB/s | 7.48 M/s | 11.09 M/s | 7.48 M/s | 11.09 M/s |
| `fstree.encode_file_node` | children-1024 | encoded bytes | 828.9 MiB/s | 2397.2 MiB/s | 25.56 M/s | 73.92 M/s | 25.56 M/s | 73.92 M/s |
| `fstree.encode_file_node` | children-128 | encoded bytes | 674.8 MiB/s | 1730.0 MiB/s | 20.80 M/s | 53.33 M/s | 20.80 M/s | 53.33 M/s |
| `fstree.encode_file_node` | children-65536 | encoded bytes | 763.9 MiB/s | 2604.0 MiB/s | 23.56 M/s | 80.31 M/s | 23.56 M/s | 80.31 M/s |
| `fstree.encode_file_node` | children-8 | encoded bytes | 297.6 MiB/s | 712.5 MiB/s | 9.14 M/s | 21.89 M/s | 9.14 M/s | 21.89 M/s |
| `fstree.encode_xattr_set` | xattrs-64 | — | — | — | 2.34 M/s | 10.30 M/s | — | — |
| `fstree.index_builder_file` | children-1024 | — | — | — | 7.69 M/s | 14.22 M/s | 7.69 M/s | 14.22 M/s |
| `fstree.index_builder_file` | children-128 | — | — | — | 5.08 M/s | 15.03 M/s | 5.08 M/s | 15.03 M/s |
| `fstree.index_builder_file` | children-65536 | — | — | — | 8.46 M/s | 14.58 M/s | 8.46 M/s | 14.58 M/s |
| `fstree.index_builder_file` | children-8 | — | — | — | 819.76 k/s | 14.52 M/s | 819.76 k/s | 14.52 M/s |
| `fstree.list_entries` | widest/first-page-100 | — | — | — | 724.80 k/s | 1.19 M/s | 724.80 k/s | 1.19 M/s |
| `fstree.list_entries` | width-16/full-paging-100 | — | — | — | 1.79 M/s | 4.52 M/s | 1.79 M/s | 4.52 M/s |
| `fstree.list_entries` | width-256/full-paging-100 | — | — | — | 1.52 M/s | 2.51 M/s | 1.52 M/s | 2.51 M/s |
| `fstree.list_entries` | width-40000/full-paging-100 | — | — | — | 736.79 k/s | 1.23 M/s | 736.79 k/s | 1.23 M/s |
| `fstree.list_entries` | width-4096/full-paging-100 | — | — | — | 868.43 k/s | 1.45 M/s | 868.43 k/s | 1.45 M/s |
| `fstree.lookup_entry` | width-16/hit | — | — | — | 197.25 k/s | 336.81 k/s | — | — |
| `fstree.lookup_entry` | width-16/miss | — | — | — | 185.41 k/s | 336.45 k/s | — | — |
| `fstree.lookup_entry` | width-256/hit | — | — | — | 23.91 k/s | 40.05 k/s | — | — |
| `fstree.lookup_entry` | width-256/miss | — | — | — | 1.56 M/s | 4.71 M/s | — | — |
| `fstree.lookup_entry` | width-40000/hit | — | — | — | 9.70 k/s | 16.17 k/s | — | — |
| `fstree.lookup_entry` | width-40000/miss | — | — | — | 1.56 M/s | 4.66 M/s | — | — |
| `fstree.lookup_entry` | width-4096/hit | — | — | — | 12.50 k/s | 20.76 k/s | — | — |
| `fstree.lookup_entry` | width-4096/miss | — | — | — | 266.23 k/s | 466.86 k/s | — | — |
| `fstree.reachable_keys` | deep | — | — | — | 166.82 k/s | 29.46 k/s | — | — |
| `fstree.reachable_keys` | file-corpus | — | — | — | 1.72 M/s | 434.34 k/s | — | — |
| `fstree.reachable_keys` | width-16 | — | — | — | 791.03 k/s | 158.93 k/s | — | — |
| `fstree.reachable_keys` | width-256 | — | — | — | 2.91 M/s | 1.43 M/s | — | — |
| `fstree.reachable_keys` | width-40000 | — | — | — | 8.34 M/s | 9.84 M/s | — | — |
| `fstree.reachable_keys` | width-4096 | — | — | — | 6.99 M/s | 6.70 M/s | — | — |
| `fstree.resolve_entry` | depth-1 | — | — | — | 161.22 k/s | 289.56 k/s | — | — |
| `fstree.resolve_entry` | depth-12 | — | — | — | 62.75 k/s | 121.55 k/s | — | — |
| `fstree.resolve_entry` | depth-24 | — | — | — | 37.62 k/s | 73.68 k/s | — | — |
| `fstree.resolve_entry` | depth-4 | — | — | — | 111.94 k/s | 211.63 k/s | — | — |
| `fstree.resolve_path` | depth-1 | — | — | — | 1.12 M/s | 2.12 M/s | — | — |
| `fstree.resolve_path` | depth-12 | — | — | — | 98.21 k/s | 191.65 k/s | — | — |
| `fstree.resolve_path` | depth-24 | — | — | — | 48.91 k/s | 96.67 k/s | — | — |
| `fstree.resolve_path` | depth-4 | — | — | — | 297.90 k/s | 566.05 k/s | — | — |
| `fstree.write_content` | file-corpus | payload bytes | 6060606.1 MiB/s | 39097.7 MiB/s | 6.06 M/s | 39.10 k/s | — | — |
| `gc.close` | idle | — | — | — | 3.02 M/s | 1.26 M/s | — | — |
| `gc.open` | populated | — | — | — | 22.44 k/s | 31.69 k/s | — | — |
| `gc.prepare_ref` | missing-root | — | — | — | 396.39 k/s | 5.99 M/s | — | — |
| `gc.prepare_ref` | tree-root/abort | — | — | — | 346 /s | 592 /s | — | — |
| `gc.prepare_ref` | tree-root/commit | — | — | — | 353 /s | 584 /s | — | — |
| `gc.release_ref` | batch | — | — | — | 154.92 M/s | 1.21 G/s | — | — |
| `gc.run` | nothing-to-reclaim | — | — | — | 266 /s | 619 /s | — | — |
| `gc.run` | reclaimable-packs | — | — | — | 67 /s | 87 /s | — | — |
| `gc.status` | mark+score | — | — | — | 189 /s | 263 /s | — | — |
| `gc.why` | live-root | — | — | — | 18.05 k/s | 174.49 k/s | — | — |
| `gc.wipe` | store-reset | — | — | — | 152 /s | 203 /s | — | — |
| `inbox.close` | drained | — | — | — | 156 /s | 162 /s | — | — |
| `inbox.commit` | packs/duplicate | — | — | — | 50.66 k/s | 48.44 k/s | — | — |
| `inbox.discard` | packs | — | — | — | 57.50 k/s | 55.88 k/s | — | — |
| `inbox.drain` | packs/new | payload bytes | 913.2 MiB/s | 1136.9 MiB/s | 3.39 k/s | 4.22 k/s | — | — |
| `inbox.open` | empty | — | — | — | 15.16 k/s | 9.62 k/s | — | — |
| `inbox.open` | sweeps-staged-tmp-files | — | — | — | 55.35 k/s | 40.64 k/s | — | — |
| `inbox.stage` | packs | payload bytes | 1512.0 MiB/s | 1480.3 MiB/s | 5.61 k/s | 5.50 k/s | — | — |
| `ingest.dir` | single-file/jobs-1 | included payload bytes | 287.7 MiB/s | 502.2 MiB/s | 36 /s | 63 /s | — | — |
| `ingest.dir` | tree/fresh-store/jobs-1 | included payload bytes | 84.8 MiB/s | 478.6 MiB/s | 8.64 k/s | 48.77 k/s | — | — |
| `ingest.dir` | tree/fresh-store/jobs-N | included payload bytes | 107.2 MiB/s | 937.9 MiB/s | 10.92 k/s | 95.57 k/s | — | — |
| `ingest.dir` | tree/incremental-change/jobs-1 | included payload bytes | 105.3 MiB/s | 993.0 MiB/s | 10.73 k/s | 101.16 k/s | — | — |
| `ingest.dir` | tree/incremental-change/jobs-N | included payload bytes | 110.4 MiB/s | 1318.8 MiB/s | 11.25 k/s | 134.35 k/s | — | — |
| `ingest.dir` | tree/unchanged-repeat/jobs-1 | included payload bytes | 105.0 MiB/s | 1005.5 MiB/s | 10.70 k/s | 102.46 k/s | — | — |
| `ingest.dir` | tree/unchanged-repeat/jobs-N | included payload bytes | 111.9 MiB/s | 1275.5 MiB/s | 11.40 k/s | 129.98 k/s | — | — |
| `ingest.objects` | tree/jobs-1 | included payload bytes | 105.0 MiB/s | 1005.1 MiB/s | 10.70 k/s | 102.42 k/s | — | — |
| `ingest.objects` | tree/jobs-N | included payload bytes | 112.5 MiB/s | 1372.0 MiB/s | 11.47 k/s | 139.81 k/s | — | — |
| `ingest.scan` | filtered/jobs-1 | logical bytes scanned | 4381.0 MiB/s | 3709.6 MiB/s | 446.44 k/s | 378.02 k/s | — | — |
| `ingest.scan` | filtered/jobs-N | logical bytes scanned | 10075.7 MiB/s | 1848.2 MiB/s | 1.03 M/s | 188.33 k/s | — | — |
| `ingest.scan` | unfiltered/jobs-1 | logical bytes scanned | 7316.1 MiB/s | 6107.0 MiB/s | 762.99 k/s | 636.89 k/s | — | — |
| `key.accessors` | type+length+length_size+hash | — | — | — | 79.74 M/s | 120.61 M/s | — | — |
| `key.new` | 1MiB-duplicate | payload bytes | 5254.5 MiB/s | 7543.1 MiB/s | 5.25 k/s | 7.54 k/s | — | — |
| `key.new` | 1MiB-random | payload bytes | 5195.1 MiB/s | 8271.7 MiB/s | 5.20 k/s | 8.27 k/s | — | — |
| `key.new` | 1MiB-text | payload bytes | 5202.5 MiB/s | 8146.4 MiB/s | 5.20 k/s | 8.15 k/s | — | — |
| `key.new` | 4KiB-duplicate | payload bytes | 2217.0 MiB/s | 3652.0 MiB/s | 567.55 k/s | 934.90 k/s | — | — |
| `key.new` | 4KiB-random | payload bytes | 2188.0 MiB/s | 3651.6 MiB/s | 560.13 k/s | 934.82 k/s | — | — |
| `key.new` | 4KiB-text | payload bytes | 2171.6 MiB/s | 3639.2 MiB/s | 555.93 k/s | 931.64 k/s | — | — |
| `key.new` | empty | payload bytes | — | — | 9.35 M/s | 12.90 M/s | — | — |
| `key.new` | large-8MiB-duplicate | payload bytes | 5260.4 MiB/s | 8421.8 MiB/s | 658 /s | 1.05 k/s | — | — |
| `key.new` | large-8MiB-random | payload bytes | 5188.8 MiB/s | 8403.0 MiB/s | 649 /s | 1.05 k/s | — | — |
| `key.new` | large-8MiB-text | payload bytes | 5237.1 MiB/s | 8389.2 MiB/s | 655 /s | 1.05 k/s | — | — |
| `key.new` | tiny-64B-duplicate | payload bytes | 569.7 MiB/s | 741.1 MiB/s | 9.33 M/s | 12.14 M/s | — | — |
| `key.new` | tiny-64B-random | payload bytes | 569.6 MiB/s | 745.6 MiB/s | 9.33 M/s | 12.22 M/s | — | — |
| `key.new` | tiny-64B-text | payload bytes | 571.5 MiB/s | 747.6 MiB/s | 9.36 M/s | 12.25 M/s | — | — |
| `key.new_from_hash` | batch | — | — | — | 24.29 M/s | 28.31 M/s | — | — |
| `key.parse` | canonical | — | — | — | 31.27 M/s | 40.14 M/s | — | — |
| `key.parse` | malformed | — | — | — | 17.44 M/s | 1.22 G/s | — | — |
| `key.string` | hex | — | — | — | 11.39 M/s | 1.94 M/s | — | — |
| `key.type_string` | names | — | — | — | 82.04 M/s | 69.46 M/s | — | — |
| `key.validate` | canonical | — | — | — | 151.46 M/s | 1.13 G/s | — | — |
| `packstore.append_record` | pre-encoded+sync | payload bytes | 4854.5 MiB/s | 4305.1 MiB/s | 150.07 k/s | 133.08 k/s | — | — |
| `packstore.barrier` | begin+observe+abort | — | — | — | 18.65 M/s | 20.29 M/s | — | — |
| `packstore.close` | populated | — | — | — | 905 /s | 1.06 k/s | — | — |
| `packstore.compact` | 90-percent-dead | — | — | — | 2.13 M/s | 2.65 M/s | — | — |
| `packstore.compact` | nothing-dead | — | — | — | 13.34 M/s | 25.46 M/s | — | — |
| `packstore.get` | hit | — | — | — | 140.67 k/s | 224.09 k/s | — | — |
| `packstore.get` | miss | — | — | — | 18.51 M/s | 19.54 M/s | — | — |
| `packstore.get_record` | hit | — | — | — | 2.73 M/s | 4.61 M/s | — | — |
| `packstore.has` | hit | — | — | — | 10.77 M/s | 16.78 M/s | — | — |
| `packstore.has` | miss | — | — | — | 19.99 M/s | 26.85 M/s | — | — |
| `packstore.has_outside` | sealed-segment | — | — | — | 12.10 M/s | 24.67 M/s | — | — |
| `packstore.liveness` | tenth-live | — | — | — | 1.38 k/s | 1.68 k/s | — | — |
| `packstore.mark_set_contains` | all-keys | — | — | — | 7.77 M/s | 8.00 M/s | — | — |
| `packstore.mark_set_mark` | all-keys | — | — | — | 14.19 M/s | 15.72 M/s | — | — |
| `packstore.missing` | half-present | — | — | — | 11.78 M/s | 8.16 M/s | — | — |
| `packstore.new_mark_set` | snapshot | — | — | — | 7.20 k/s | 16.74 k/s | — | — |
| `packstore.oldest_inflight_write` | idle | — | — | — | 74.53 M/s | 311.84 M/s | — | — |
| `packstore.open` | empty | — | — | — | 37.78 k/s | 99.12 k/s | — | — |
| `packstore.open` | populated-reopen | — | — | — | 884 /s | 711 /s | — | — |
| `packstore.put` | 1MiB-duplicate/sync-off | payload bytes | 1946.9 MiB/s | 3689.0 MiB/s | 1.95 k/s | 3.69 k/s | — | — |
| `packstore.put` | 1MiB-random/sync-off | payload bytes | 1024.9 MiB/s | 1913.8 MiB/s | 1.02 k/s | 1.91 k/s | — | — |
| `packstore.put` | 1MiB-text/sync-off | payload bytes | 267.8 MiB/s | 507.0 MiB/s | 268 /s | 507 /s | — | — |
| `packstore.put` | 4KiB-duplicate/sync-off | payload bytes | 5637.6 MiB/s | 7101.8 MiB/s | 1.44 M/s | 1.82 M/s | — | — |
| `packstore.put` | 4KiB-random/already-present | payload bytes | 41692.7 MiB/s | 51956.1 MiB/s | 10.67 M/s | 13.30 M/s | — | — |
| `packstore.put` | 4KiB-random/sync-off | payload bytes | 543.7 MiB/s | 515.8 MiB/s | 139.18 k/s | 132.05 k/s | — | — |
| `packstore.put` | 4KiB-random/sync-on | payload bytes | 42.6 MiB/s | 42.7 MiB/s | 10.90 k/s | 10.93 k/s | — | — |
| `packstore.put` | 4KiB-text/sync-off | payload bytes | 145.1 MiB/s | 241.8 MiB/s | 37.14 k/s | 61.90 k/s | — | — |
| `packstore.put` | empty/sync-off | payload bytes | — | — | 9.92 M/s | 11.39 M/s | — | — |
| `packstore.put` | large-8MiB-duplicate/sync-off | payload bytes | 2439.4 MiB/s | 5077.7 MiB/s | 305 /s | 635 /s | — | — |
| `packstore.put` | large-8MiB-random/sync-off | payload bytes | 1198.0 MiB/s | 2581.5 MiB/s | 150 /s | 323 /s | — | — |
| `packstore.put` | large-8MiB-text/sync-off | payload bytes | 298.9 MiB/s | 551.9 MiB/s | 37 /s | 69 /s | — | — |
| `packstore.put` | tiny-64B-duplicate/sync-off | payload bytes | 663.2 MiB/s | 736.2 MiB/s | 10.87 M/s | 12.06 M/s | — | — |
| `packstore.put` | tiny-64B-random/sync-off | payload bytes | 35.8 MiB/s | 36.9 MiB/s | 587.25 k/s | 605.14 k/s | — | — |
| `packstore.put` | tiny-64B-text/sync-off | payload bytes | 26.3 MiB/s | 27.6 MiB/s | 431.67 k/s | 451.70 k/s | — | — |
| `packstore.put_verified` | already-intact | — | — | — | 11.38 k/s | 18.76 k/s | — | — |
| `packstore.record` | by-location | — | — | — | 2.17 M/s | 1.68 M/s | — | — |
| `packstore.remove` | one-sealed-segment | — | — | — | 1.22 k/s | 1.59 k/s | — | — |
| `packstore.scan_index` | all-segments | — | — | — | 34.13 M/s | 41.78 M/s | — | — |
| `packstore.segments` | list | — | — | — | 140.37 k/s | 163.97 k/s | — | — |
| `packstore.sort_by_location` | scattered | — | — | — | 5.25 M/s | 11.01 M/s | — | — |
| `packstore.stored_size` | hit | — | — | — | 10.45 M/s | 18.74 M/s | — | — |
| `packstore.verify` | full-scrub | — | — | — | 108.01 k/s | 169.54 k/s | — | — |
| `packstore.wipe` | populated | — | — | — | 192 /s | 261 /s | — | — |
| `packstore.write_batch` | mixed-objects | payload bytes | 262.7 MiB/s | 399.1 MiB/s | 31.19 k/s | 47.38 k/s | — | — |
| `packstore.write_parallel` | duplicate-stream/writers-N | payload bytes | 25692.0 MiB/s | 7084.0 MiB/s | 3.05 M/s | 841.05 k/s | — | — |
| `packstore.write_parallel` | mixed-objects/writers-1/verify-false | payload bytes | 259.6 MiB/s | 389.7 MiB/s | 30.82 k/s | 46.26 k/s | — | — |
| `packstore.write_parallel` | mixed-objects/writers-1/verify-true | payload bytes | 240.0 MiB/s | 362.4 MiB/s | 28.49 k/s | 43.03 k/s | — | — |
| `packstore.write_parallel` | mixed-objects/writers-N/verify-false | payload bytes | 923.1 MiB/s | 1543.7 MiB/s | 109.59 k/s | 183.28 k/s | — | — |
| `packstore.write_parallel` | mixed-objects/writers-N/verify-true | payload bytes | 885.0 MiB/s | 1527.2 MiB/s | 105.07 k/s | 181.32 k/s | — | — |
| `reference.decode` | signed | encoded bytes | 350.3 MiB/s | 701.5 MiB/s | 1.16 M/s | 2.32 M/s | — | — |
| `reference.encode` | signed | encoded bytes | 1088.3 MiB/s | 1801.0 MiB/s | 3.60 M/s | 5.96 M/s | — | — |
| `reference.signature_payload` | signed | — | — | — | 3.74 M/s | 5.82 M/s | — | — |
| `reference.validate_name` | valid | — | — | — | 17.54 M/s | 33.88 M/s | — | — |
| `reference.validate_user` | valid | — | — | — | 13.31 M/s | 11.10 M/s | — | — |
| `refstore.all` | records | — | — | — | 9.22 M/s | 6.19 M/s | — | — |
| `refstore.close` | populated | — | — | — | 7.50 k/s | 1.09 k/s | — | — |
| `refstore.delete` | records | — | — | — | 592.49 k/s | 25.22 k/s | — | — |
| `refstore.get` | hit | — | — | — | 974.93 k/s | 1.79 M/s | — | — |
| `refstore.get` | miss | — | — | — | 4.76 M/s | 2.90 M/s | — | — |
| `refstore.open` | empty | — | — | — | 1.34 k/s | 787 /s | — | — |
| `refstore.open` | populated-reopen | — | — | — | 407 /s | 1.11 k/s | — | — |
| `refstore.put` | records/sync-false | — | — | — | 2.10 M/s | 22.81 k/s | — | — |
| `refstore.put` | records/sync-true | — | — | — | 16.60 k/s | 22.73 k/s | — | — |
| `refstore.put_batch` | records/sync-false | — | — | — | 8.65 M/s | 1.11 M/s | — | — |
| `refstore.wipe` | records | — | — | — | 855 /s | 61 /s | — | — |
| `tarexport.write` | fixture-tree | encoded bytes | 4886.3 MiB/s | 5081.6 MiB/s | 424.19 k/s | 441.15 k/s | — | — |
| `tarextract.extract` | fixture-tree | encoded bytes | 292.6 MiB/s | 357.0 MiB/s | 25.40 k/s | 30.99 k/s | — | — |

## Scaling

Each row is one dimension swept with everything else about the case held
fixed, so the trend is a statement about that dimension. `per unit` is the
time divided by the swept dimension at its largest point: where it is flat
across the row the cost is proportional, where it falls the fixed overhead
dominated the small end.

### `amberpack.encode_record` over object size (bytes) (duplicate)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-duplicate | 513.5 ns | 619.0 ns | 0.83× | 8.0 ns | 9.7 ns |
| 4,096 | 4KiB-duplicate | 3.492 us | 4.399 us | 0.79× | 0.9 ns | 1.1 ns |
| 1,048,576 | 1MiB-duplicate | 439.512 us | 259.699 us | 1.69× | 0.4 ns | 0.2 ns |
| 8,388,608 | large-8MiB-duplicate | 3.811 ms | 2.072 ms | 1.84× | 0.5 ns | 0.2 ns |

### `amberpack.encode_record` over object size (bytes) (random)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-random | 562.2 ns | 622.3 ns | 0.90× | 8.8 ns | 9.7 ns |
| 4,096 | 4KiB-random | 3.668 us | 4.500 us | 0.82× | 0.9 ns | 1.1 ns |
| 1,048,576 | 1MiB-random | 384.344 us | 258.148 us | 1.49× | 0.4 ns | 0.2 ns |
| 8,388,608 | large-8MiB-random | 3.668 ms | 2.077 ms | 1.77× | 0.4 ns | 0.2 ns |

### `amberpack.encode_record` over object size (bytes) (text)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-text | 1.135 us | 1.161 us | 0.98× | 17.7 ns | 18.1 ns |
| 4,096 | 4KiB-text | 24.287 us | 14.217 us | 1.71× | 5.9 ns | 3.5 ns |
| 1,048,576 | 1MiB-text | 3.175 ms | 1.778 ms | 1.79× | 3.0 ns | 1.7 ns |
| 8,388,608 | large-8MiB-text | 25.683 ms | 14.086 ms | 1.82× | 3.1 ns | 1.7 ns |

### `fstree.check_complete` over directory width (plain)

| directory width | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per width | Rust ns per width |
|---:|---|---:|---:|---:|---:|---:|
| 16 | width-16/jobs-1 | 1.548 us | 313.5 ns | 4.94× | 1.644 us | 333.1 ns |
| 256 | width-256/jobs-1 | 751.5 ns | 268.4 ns | 2.80× | 760.3 ns | 271.6 ns |
| 4,096 | width-4096/jobs-1 | 661.9 ns | 276.2 ns | 2.40× | 666.1 ns | 278.0 ns |
| 40,000 | width-40000/jobs-1 | 670.7 ns | 273.4 ns | 2.45× | 675.1 ns | 275.1 ns |

### `fstree.collect_entries` over directory width (plain)

| directory width | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per width | Rust ns per width |
|---:|---|---:|---:|---:|---:|---:|
| 16 | width-16 | 677.5 ns | 242.3 ns | 2.80× | 677.5 ns | 242.3 ns |
| 256 | width-256 | 379.0 ns | 219.8 ns | 1.72× | 379.0 ns | 219.8 ns |
| 4,096 | width-4096 | 365.5 ns | 223.8 ns | 1.63× | 365.5 ns | 223.8 ns |
| 40,000 | width-40000 | 397.1 ns | 221.3 ns | 1.79× | 397.1 ns | 221.3 ns |

### `fstree.decode_dir_leaf` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 8 | entries-8 | 374.3 ns | 215.4 ns | 1.74× | 374.3 ns | 215.4 ns |
| 128 | entries-128 | 356.2 ns | 212.8 ns | 1.67× | 356.2 ns | 212.8 ns |
| 1,024 | entries-1024 | 350.8 ns | 215.4 ns | 1.63× | 350.8 ns | 215.4 ns |

### `fstree.decode_dir_node` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 8 | pairs-8 | 187.6 ns | 85.9 ns | 2.18× | 187.6 ns | 85.9 ns |
| 128 | pairs-128 | 167.5 ns | 96.3 ns | 1.74× | 167.5 ns | 96.3 ns |
| 1,024 | pairs-1024 | 169.3 ns | 99.5 ns | 1.70× | 169.3 ns | 99.5 ns |

### `fstree.decode_file_node` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 8 | children-8 | 110.5 ns | 63.1 ns | 1.75× | 110.5 ns | 63.1 ns |
| 128 | children-128 | 87.4 ns | 63.0 ns | 1.39× | 87.4 ns | 63.0 ns |
| 1,024 | children-1024 | 86.0 ns | 69.0 ns | 1.25× | 86.0 ns | 69.0 ns |
| 65,536 | children-65536 | 89.0 ns | 69.8 ns | 1.28× | 89.0 ns | 69.8 ns |

### `fstree.dir_builder` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 16 | entries-16 | 1.363 us | 333.1 ns | 4.09× | 1.363 us | 333.1 ns |
| 256 | entries-256 | 545.6 ns | 334.6 ns | 1.63× | 545.6 ns | 334.6 ns |
| 4,096 | entries-4096 | 482.2 ns | 320.1 ns | 1.51× | 482.2 ns | 320.1 ns |
| 40,000 | entries-40000 | 496.6 ns | 332.9 ns | 1.49× | 496.6 ns | 332.9 ns |

### `fstree.encode_blob` over object size (bytes) (duplicate)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-duplicate | 135.3 ns | 107.2 ns | 1.26× | 2.1 ns | 1.7 ns |
| 4,096 | 4KiB-duplicate | 1.786 us | 1.122 us | 1.59× | 0.4 ns | 0.3 ns |
| 1,048,576 | 1MiB-duplicate | 192.751 us | 135.028 us | 1.43× | 0.2 ns | 0.1 ns |
| 8,388,608 | large-8MiB-duplicate | 1.516 ms | 1.099 ms | 1.38× | 0.2 ns | 0.1 ns |

### `fstree.encode_blob` over object size (bytes) (random)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-random | 135.8 ns | 107.7 ns | 1.26× | 2.1 ns | 1.7 ns |
| 4,096 | 4KiB-random | 1.817 us | 1.121 us | 1.62× | 0.4 ns | 0.3 ns |
| 1,048,576 | 1MiB-random | 202.469 us | 134.101 us | 1.51× | 0.2 ns | 0.1 ns |
| 8,388,608 | large-8MiB-random | 1.528 ms | 1.122 ms | 1.36× | 0.2 ns | 0.1 ns |

### `fstree.encode_blob` over object size (bytes) (text)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-text | 135.4 ns | 108.4 ns | 1.25× | 2.1 ns | 1.7 ns |
| 4,096 | 4KiB-text | 1.916 us | 1.127 us | 1.70× | 0.5 ns | 0.3 ns |
| 1,048,576 | 1MiB-text | 205.398 us | 135.251 us | 1.52× | 0.2 ns | 0.1 ns |
| 8,388,608 | large-8MiB-text | 1.524 ms | 1.119 ms | 1.36× | 0.2 ns | 0.1 ns |

### `fstree.encode_dir_leaf` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 8 | entries-8 | 235.0 ns | 129.7 ns | 1.81× | 235.0 ns | 129.7 ns |
| 128 | entries-128 | 165.4 ns | 59.4 ns | 2.78× | 165.4 ns | 59.4 ns |
| 1,024 | entries-1024 | 152.2 ns | 50.4 ns | 3.02× | 152.2 ns | 50.4 ns |

### `fstree.encode_dir_node` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 8 | pairs-8 | 133.8 ns | 90.2 ns | 1.48× | 133.8 ns | 90.2 ns |
| 128 | pairs-128 | 70.2 ns | 42.9 ns | 1.64× | 70.2 ns | 42.9 ns |
| 1,024 | pairs-1024 | 65.2 ns | 31.6 ns | 2.06× | 65.2 ns | 31.6 ns |

### `fstree.encode_file_node` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 8 | children-8 | 109.4 ns | 45.7 ns | 2.39× | 109.4 ns | 45.7 ns |
| 128 | children-128 | 48.1 ns | 18.8 ns | 2.56× | 48.1 ns | 18.8 ns |
| 1,024 | children-1024 | 39.1 ns | 13.5 ns | 2.89× | 39.1 ns | 13.5 ns |
| 65,536 | children-65536 | 42.4 ns | 12.5 ns | 3.41× | 42.4 ns | 12.5 ns |

### `fstree.index_builder_file` over entries (plain)

| entries | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per entries | Rust ns per entries |
|---:|---|---:|---:|---:|---:|---:|
| 8 | children-8 | 1.220 us | 68.9 ns | 17.71× | 1.220 us | 68.9 ns |
| 128 | children-128 | 197.0 ns | 66.5 ns | 2.96× | 197.0 ns | 66.5 ns |
| 1,024 | children-1024 | 130.0 ns | 70.3 ns | 1.85× | 130.0 ns | 70.3 ns |
| 65,536 | children-65536 | 118.2 ns | 68.6 ns | 1.72× | 118.2 ns | 68.6 ns |

### `fstree.list_entries` over directory width (plain)

| directory width | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per width | Rust ns per width |
|---:|---|---:|---:|---:|---:|---:|
| 16 | width-16/full-paging-100 | 557.3 ns | 221.0 ns | 2.52× | 557.3 ns | 221.0 ns |
| 256 | width-256/full-paging-100 | 657.6 ns | 399.2 ns | 1.65× | 657.6 ns | 399.2 ns |
| 4,096 | width-4096/full-paging-100 | 1.151 us | 690.2 ns | 1.67× | 1.151 us | 690.2 ns |
| 40,000 | width-40000/full-paging-100 | 1.357 us | 810.8 ns | 1.67× | 1.357 us | 810.8 ns |

### `fstree.lookup_entry` over directory width (hit)

| directory width | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per width | Rust ns per width |
|---:|---|---:|---:|---:|---:|---:|
| 16 | width-16/hit | 5.070 us | 2.969 us | 1.71× | 633.729 us | 371.133 us |
| 256 | width-256/hit | 41.832 us | 24.972 us | 1.68× | 326.810 us | 195.091 us |
| 4,096 | width-4096/hit | 79.979 us | 48.160 us | 1.66× | 39.052 us | 23.516 us |
| 40,000 | width-40000/hit | 103.122 us | 61.831 us | 1.67× | 5.156 us | 3.092 us |

### `fstree.lookup_entry` over directory width (miss)

| directory width | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per width | Rust ns per width |
|---:|---|---:|---:|---:|---:|---:|
| 16 | width-16/miss | 5.393 us | 2.972 us | 1.81× | 674.171 us | 371.527 us |
| 256 | width-256/miss | 639.0 ns | 212.5 ns | 3.01× | 4.992 us | 1.660 us |
| 4,096 | width-4096/miss | 3.756 us | 2.142 us | 1.75× | 1.834 us | 1.046 us |
| 40,000 | width-40000/miss | 639.7 ns | 214.8 ns | 2.98× | 32.0 ns | 10.7 ns |

### `fstree.reachable_keys` over directory width (plain)

| directory width | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per width | Rust ns per width |
|---:|---|---:|---:|---:|---:|---:|
| 16 | width-16 | 1.264 us | 6.292 us | 0.20× | 1.343 us | 6.685 us |
| 256 | width-256 | 343.5 ns | 699.2 ns | 0.49× | 347.5 ns | 707.4 ns |
| 4,096 | width-4096 | 143.1 ns | 149.2 ns | 0.96× | 144.0 ns | 150.2 ns |
| 40,000 | width-40000 | 120.0 ns | 101.6 ns | 1.18× | 120.7 ns | 102.3 ns |

### `fstree.resolve_entry` over path depth (plain)

| path depth | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per depth | Rust ns per depth |
|---:|---|---:|---:|---:|---:|---:|
| 1 | depth-1 | 6.203 us | 3.453 us | 1.80× | 6.203 us | 3.453 us |
| 4 | depth-4 | 8.933 us | 4.725 us | 1.89× | 2.233 us | 1.181 us |
| 12 | depth-12 | 15.935 us | 8.227 us | 1.94× | 1.328 us | 685.6 ns |
| 24 | depth-24 | 26.578 us | 13.572 us | 1.96× | 1.107 us | 565.5 ns |

### `fstree.resolve_path` over path depth (plain)

| path depth | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per depth | Rust ns per depth |
|---:|---|---:|---:|---:|---:|---:|
| 1 | depth-1 | 894.0 ns | 470.9 ns | 1.90× | 894.0 ns | 470.9 ns |
| 4 | depth-4 | 3.357 us | 1.767 us | 1.90× | 839.2 ns | 441.7 ns |
| 12 | depth-12 | 10.182 us | 5.218 us | 1.95× | 848.5 ns | 434.8 ns |
| 24 | depth-24 | 20.444 us | 10.344 us | 1.98× | 851.8 ns | 431.0 ns |

### `key.new` over object size (bytes) (duplicate)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-duplicate | 107.1 ns | 82.4 ns | 1.30× | 1.7 ns | 1.3 ns |
| 4,096 | 4KiB-duplicate | 1.762 us | 1.070 us | 1.65× | 0.4 ns | 0.3 ns |
| 1,048,576 | 1MiB-duplicate | 190.315 us | 132.571 us | 1.44× | 0.2 ns | 0.1 ns |
| 8,388,608 | large-8MiB-duplicate | 1.521 ms | 949.919 us | 1.60× | 0.2 ns | 0.1 ns |

### `key.new` over object size (bytes) (random)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-random | 107.2 ns | 81.9 ns | 1.31× | 1.7 ns | 1.3 ns |
| 4,096 | 4KiB-random | 1.785 us | 1.070 us | 1.67× | 0.4 ns | 0.3 ns |
| 1,048,576 | 1MiB-random | 192.488 us | 120.894 us | 1.59× | 0.2 ns | 0.1 ns |
| 8,388,608 | large-8MiB-random | 1.542 ms | 952.039 us | 1.62× | 0.2 ns | 0.1 ns |

### `key.new` over object size (bytes) (text)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-text | 106.8 ns | 81.6 ns | 1.31× | 1.7 ns | 1.3 ns |
| 4,096 | 4KiB-text | 1.799 us | 1.073 us | 1.68× | 0.4 ns | 0.3 ns |
| 1,048,576 | 1MiB-text | 192.216 us | 122.753 us | 1.57× | 0.2 ns | 0.1 ns |
| 8,388,608 | large-8MiB-text | 1.528 ms | 953.602 us | 1.60× | 0.2 ns | 0.1 ns |

### `packstore.put` over object size (bytes) (duplicate)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-duplicate/sync-off | 92.0 ns | 82.9 ns | 1.11× | 1.4 ns | 1.3 ns |
| 4,096 | 4KiB-duplicate/sync-off | 692.9 ns | 550.0 ns | 1.26× | 0.2 ns | 0.1 ns |
| 1,048,576 | 1MiB-duplicate/sync-off | 513.641 us | 271.078 us | 1.89× | 0.5 ns | 0.3 ns |
| 8,388,608 | large-8MiB-duplicate/sync-off | 3.280 ms | 1.576 ms | 2.08× | 0.4 ns | 0.2 ns |

### `packstore.put` over object size (bytes) (text)

| object size (bytes) | Workload | Go ns/op | Rust ns/op | Go/Rust | Go ns per item_bytes | Rust ns per item_bytes |
|---:|---|---:|---:|---:|---:|---:|
| 64 | tiny-64B-text/sync-off | 2.317 us | 2.214 us | 1.05× | 36.2 ns | 34.6 ns |
| 4,096 | 4KiB-text/sync-off | 26.925 us | 16.155 us | 1.67× | 6.6 ns | 3.9 ns |
| 1,048,576 | 1MiB-text/sync-off | 3.734 ms | 1.973 ms | 1.89× | 3.6 ns | 1.9 ns |
| 8,388,608 | large-8MiB-text/sync-off | 26.765 ms | 14.494 ms | 1.85× | 3.2 ns | 1.7 ns |

## Worker scaling

The same operation asked for one worker and for the profile's concurrent
count. Speed-up is the single-worker time divided by the concurrent time;
both cores were pinned to the same CPU set.

| Operation | Workload | Workers | Go 1 | Go n | Go speed-up | Rust 1 | Rust n | Rust speed-up |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| `fstree.check_complete` | widest/jobs-* | 8 | 668.5 ns | 350.7 ns | 1.91× | 272.5 ns | 129.3 ns | 2.11× |
| `ingest.dir` | tree/fresh-store/jobs-* | 8 | 115.712 us | 91.535 us | 1.26× | 20.505 us | 10.463 us | 1.96× |
| `ingest.dir` | tree/incremental-change/jobs-* | 8 | 93.227 us | 88.876 us | 1.05× | 9.885 us | 7.443 us | 1.33× |
| `ingest.dir` | tree/unchanged-repeat/jobs-* | 8 | 93.500 us | 87.694 us | 1.07× | 9.760 us | 7.694 us | 1.27× |
| `ingest.objects` | tree/jobs-* | 8 | 93.455 us | 87.217 us | 1.07× | 9.763 us | 7.152 us | 1.37× |
| `ingest.scan` | filtered/jobs-* | 8 | 2.240 us | 974.0 ns | 2.30× | 2.645 us | 5.310 us | 0.50× |
| `packstore.write_parallel` | mixed-objects/writers-*/verify-false | 8 | 32.445 us | 9.125 us | 3.56× | 21.616 us | 5.456 us | 3.96× |
| `packstore.write_parallel` | mixed-objects/writers-*/verify-true | 8 | 35.095 us | 9.517 us | 3.69× | 23.242 us | 5.515 us | 4.21× |

## CPU time

Process CPU time (user + system) attributed to the measured interval, per
operation. It is the one resource figure the two runtimes report the same
way; for concurrent cases it exceeds the elapsed time.

| Operation | Workload | Thr | Go CPU ns/op | Rust CPU ns/op | Go/Rust |
|---|---|---|---:|---:|---:|
| `amberignore.descend` | 8-subdirs | 1 | 5.168 us | 3.146 us | 1.64× |
| `amberignore.ignored` | mixed-names | 1 | 214.8 ns | 119.1 ns | 1.80× |
| `amberignore.ignored` | nil-matcher | 1 | 9.8 ns | 0.0 ns | — |
| `amberignore.root` | load-root | 1 | 7.375 us | 4.281 us | 1.72× |
| `amberpack.decode_payload` | mixed-records/producer-go | 1 | 17.260 us | 10.656 us | 1.62× |
| `amberpack.decode_payload` | mixed-records/producer-rust | 1 | 16.406 us | 9.724 us | 1.69× |
| `amberpack.encode_record` | 1MiB-duplicate | 1 | 440.250 us | 259.250 us | 1.70× |
| `amberpack.encode_record` | 1MiB-random | 1 | 397.750 us | 257.750 us | 1.54× |
| `amberpack.encode_record` | 1MiB-text | 1 | 3.424 ms | 1.773 ms | 1.93× |
| `amberpack.encode_record` | 4KiB-duplicate | 1 | 3.484 us | 4.382 us | 0.80× |
| `amberpack.encode_record` | 4KiB-random | 1 | 4.616 us | 4.476 us | 1.03× |
| `amberpack.encode_record` | 4KiB-text | 1 | 24.995 us | 14.166 us | 1.76× |
| `amberpack.encode_record` | empty | 1 | 62.4 ns | 289.6 ns | 0.22× |
| `amberpack.encode_record` | large-8MiB-duplicate | 1 | 4.285 ms | 2.062 ms | 2.08× |
| `amberpack.encode_record` | large-8MiB-random | 1 | 4.087 ms | 2.061 ms | 1.98× |
| `amberpack.encode_record` | large-8MiB-text | 1 | 26.385 ms | 14.025 ms | 1.88× |
| `amberpack.encode_record` | tiny-64B-duplicate | 1 | 512.6 ns | 617.1 ns | 0.83× |
| `amberpack.encode_record` | tiny-64B-random | 1 | 560.4 ns | 620.4 ns | 0.90× |
| `amberpack.encode_record` | tiny-64B-text | 1 | 1.131 us | 1.157 us | 0.98× |
| `amberpack.parse_record` | corrupt-crc/producer-go | 1 | 156.2 ns | 101.6 ns | 1.54× |
| `amberpack.parse_record` | corrupt-crc/producer-rust | 1 | 148.4 ns | 105.5 ns | 1.41× |
| `amberpack.parse_record` | mixed-records/producer-go | 1 | 736.0 ns | 2.272 us | 0.32× |
| `amberpack.parse_record` | mixed-records/producer-rust | 1 | 722.0 ns | 2.266 us | 0.32× |
| `amberpack.reader_all` | mixed-objects/producer-go | 1 | 16.436 us | 13.548 us | 1.21× |
| `amberpack.reader_all` | mixed-objects/producer-rust | 1 | 15.998 us | 12.648 us | 1.26× |
| `amberpack.reader_all` | truncated-stream/producer-go | 1 | 5.673 ms | 3.267 ms | 1.74× |
| `amberpack.reader_all` | truncated-stream/producer-rust | 1 | 5.453 ms | 3.065 ms | 1.78× |
| `amberpack.reader_records` | mixed-objects/producer-go | 1 | 1.562 us | 2.828 us | 0.55× |
| `amberpack.reader_records` | mixed-objects/producer-rust | 1 | 1.456 us | 2.810 us | 0.52× |
| `amberpack.writer_add` | mixed-objects | 1 | 70.946 us | 39.470 us | 1.80× |
| `amberpack.writer_add_record` | pre-encoded-records/producer-go | 1 | 38.0 ns | 8.0 ns | 4.75× |
| `amberpack.writer_add_record` | pre-encoded-records/producer-rust | 1 | 36.0 ns | 10.0 ns | 3.60× |
| `cbor.decode_xattrs` | xattrs-3 | 1 | 484.4 ns | 187.5 ns | 2.58× |
| `cbor.decode_xattrs` | xattrs-64 | 1 | 8.938 us | 5.547 us | 1.61× |
| `cbor.encode_xattrs` | xattrs-3 | 1 | 500.0 ns | 265.6 ns | 1.88× |
| `cbor.encode_xattrs` | xattrs-64 | 1 | 25.156 us | 4.500 us | 5.59× |
| `chunkers.item_chunker` | is_boundary/bits-7 | 1 | 54.0 ns | 33.0 ns | 1.64× |
| `chunkers.new_item_chunker` | bits-4..12 | 1 | 15.6 ns | 13.5 ns | 1.16× |
| `chunkers.split_bytes` | compressible/4-16-64KiB | 1 | 56.568 us | 40.103 us | 1.41× |
| `chunkers.split_bytes` | compressible/default-sizes | 1 | 904.891 us | 657.844 us | 1.38× |
| `chunkers.split_bytes` | random/default-sizes | 1 | 49.631 us | 44.278 us | 1.12× |
| `chunkers.split_bytes` | tiny-64KiB/default-sizes | 1 | 57.000 us | 22.000 us | 2.59× |
| `fstree.check_complete` | incomplete/jobs-1 | 1 | 41.000 us | 7.000 us | 5.86× |
| `fstree.check_complete` | widest/jobs-1 | 1 | 695.4 ns | 271.5 ns | 2.56× |
| `fstree.check_complete` | widest/jobs-N | 8 | 849.8 ns | 635.0 ns | 1.34× |
| `fstree.check_complete` | width-16/jobs-1 | 1 | 2.000 us | 352.9 ns | 5.67× |
| `fstree.check_complete` | width-256/jobs-1 | 1 | 818.5 ns | 270.3 ns | 3.03× |
| `fstree.check_complete` | width-40000/jobs-1 | 1 | 699.1 ns | 272.2 ns | 2.57× |
| `fstree.check_complete` | width-4096/jobs-1 | 1 | 695.5 ns | 275.1 ns | 2.53× |
| `fstree.child_keys` | dir-leaf-128 | 1 | 46.703 us | 29.031 us | 1.61× |
| `fstree.child_keys` | dir-node-128 | 1 | 21.219 us | 14.188 us | 1.50× |
| `fstree.child_keys` | file-node-1024 | 1 | 87.844 us | 71.297 us | 1.23× |
| `fstree.collect_entries` | width-16 | 1 | 875.0 ns | 250.0 ns | 3.50× |
| `fstree.collect_entries` | width-256 | 1 | 390.6 ns | 222.7 ns | 1.75× |
| `fstree.collect_entries` | width-40000 | 1 | 395.9 ns | 220.4 ns | 1.80× |
| `fstree.collect_entries` | width-4096 | 1 | 366.2 ns | 223.4 ns | 1.64× |
| `fstree.decode_dir_leaf` | entries-1024 | 1 | 349.9 ns | 214.6 ns | 1.63× |
| `fstree.decode_dir_leaf` | entries-128 | 1 | 355.8 ns | 211.5 ns | 1.68× |
| `fstree.decode_dir_leaf` | entries-128-partial-change | 1 | 371.1 ns | 222.7 ns | 1.67× |
| `fstree.decode_dir_leaf` | entries-128-with-xattrs | 1 | 371.0 ns | 222.3 ns | 1.67× |
| `fstree.decode_dir_leaf` | entries-8 | 1 | 386.7 ns | 216.8 ns | 1.78× |
| `fstree.decode_dir_node` | pairs-1024 | 1 | 168.9 ns | 99.3 ns | 1.70× |
| `fstree.decode_dir_node` | pairs-128 | 1 | 167.2 ns | 96.2 ns | 1.74× |
| `fstree.decode_dir_node` | pairs-8 | 1 | 201.2 ns | 87.9 ns | 2.29× |
| `fstree.decode_file_node` | children-1024 | 1 | 86.0 ns | 68.9 ns | 1.25× |
| `fstree.decode_file_node` | children-128 | 1 | 87.6 ns | 63.1 ns | 1.39× |
| `fstree.decode_file_node` | children-65536 | 1 | 89.0 ns | 69.5 ns | 1.28× |
| `fstree.decode_file_node` | children-8 | 1 | 119.1 ns | 64.5 ns | 1.85× |
| `fstree.dir_builder` | entries-16 | 1 | 1.562 us | 375.0 ns | 4.17× |
| `fstree.dir_builder` | entries-256 | 1 | 558.6 ns | 335.9 ns | 1.66× |
| `fstree.dir_builder` | entries-40000 | 1 | 518.9 ns | 331.6 ns | 1.56× |
| `fstree.dir_builder` | entries-4096 | 1 | 478.5 ns | 319.8 ns | 1.50× |
| `fstree.encode_blob` | 1MiB-duplicate | 1 | 192.500 us | 134.750 us | 1.43× |
| `fstree.encode_blob` | 1MiB-random | 1 | 226.750 us | 133.750 us | 1.70× |
| `fstree.encode_blob` | 1MiB-text | 1 | 205.000 us | 134.750 us | 1.52× |
| `fstree.encode_blob` | 4KiB-duplicate | 1 | 1.786 us | 1.118 us | 1.60× |
| `fstree.encode_blob` | 4KiB-random | 1 | 1.817 us | 1.118 us | 1.63× |
| `fstree.encode_blob` | 4KiB-text | 1 | 1.914 us | 1.119 us | 1.71× |
| `fstree.encode_blob` | empty | 1 | 122.2 ns | 89.9 ns | 1.36× |
| `fstree.encode_blob` | large-8MiB-duplicate | 1 | 1.516 ms | 1.096 ms | 1.38× |
| `fstree.encode_blob` | large-8MiB-random | 1 | 1.519 ms | 1.119 ms | 1.36× |
| `fstree.encode_blob` | large-8MiB-text | 1 | 1.528 ms | 1.116 ms | 1.37× |
| `fstree.encode_blob` | tiny-64B-duplicate | 1 | 135.3 ns | 106.9 ns | 1.27× |
| `fstree.encode_blob` | tiny-64B-random | 1 | 135.7 ns | 107.3 ns | 1.26× |
| `fstree.encode_blob` | tiny-64B-text | 1 | 135.5 ns | 108.3 ns | 1.25× |
| `fstree.encode_dir_leaf` | entries-1024 | 1 | 151.8 ns | 50.3 ns | 3.02× |
| `fstree.encode_dir_leaf` | entries-128 | 1 | 165.6 ns | 59.4 ns | 2.79× |
| `fstree.encode_dir_leaf` | entries-128-partial-change | 1 | 175.8 ns | 68.6 ns | 2.56× |
| `fstree.encode_dir_leaf` | entries-128-with-xattrs | 1 | 175.0 ns | 69.1 ns | 2.53× |
| `fstree.encode_dir_leaf` | entries-8 | 1 | 244.1 ns | 130.9 ns | 1.87× |
| `fstree.encode_dir_node` | pairs-1024 | 1 | 65.0 ns | 31.6 ns | 2.06× |
| `fstree.encode_dir_node` | pairs-128 | 1 | 70.6 ns | 43.0 ns | 1.64× |
| `fstree.encode_dir_node` | pairs-8 | 1 | 142.6 ns | 89.8 ns | 1.59× |
| `fstree.encode_file_node` | children-1024 | 1 | 39.0 ns | 13.5 ns | 2.89× |
| `fstree.encode_file_node` | children-128 | 1 | 49.0 ns | 18.8 ns | 2.60× |
| `fstree.encode_file_node` | children-65536 | 1 | 43.0 ns | 12.4 ns | 3.47× |
| `fstree.encode_file_node` | children-8 | 1 | 115.2 ns | 46.9 ns | 2.46× |
| `fstree.encode_xattr_set` | xattrs-64 | 1 | 427.0 ns | 96.9 ns | 4.41× |
| `fstree.index_builder_file` | children-1024 | 1 | 132.8 ns | 71.3 ns | 1.86× |
| `fstree.index_builder_file` | children-128 | 1 | 218.8 ns | 70.3 ns | 3.11× |
| `fstree.index_builder_file` | children-65536 | 1 | 133.3 ns | 68.3 ns | 1.95× |
| `fstree.index_builder_file` | children-8 | 1 | 1.875 us | 125.0 ns | 15.00× |
| `fstree.list_entries` | widest/first-page-100 | 1 | 1.526 us | 835.3 ns | 1.83× |
| `fstree.list_entries` | width-16/full-paging-100 | 1 | 812.5 ns | 250.0 ns | 3.25× |
| `fstree.list_entries` | width-256/full-paging-100 | 1 | 668.0 ns | 394.5 ns | 1.69× |
| `fstree.list_entries` | width-40000/full-paging-100 | 1 | 1.353 us | 807.9 ns | 1.67× |
| `fstree.list_entries` | width-4096/full-paging-100 | 1 | 1.152 us | 685.3 ns | 1.68× |
| `fstree.lookup_entry` | width-16/hit | 1 | 5.255 us | 2.958 us | 1.78× |
| `fstree.lookup_entry` | width-16/miss | 1 | 5.373 us | 2.967 us | 1.81× |
| `fstree.lookup_entry` | width-256/hit | 1 | 41.700 us | 24.897 us | 1.67× |
| `fstree.lookup_entry` | width-256/miss | 1 | 640.0 ns | 212.5 ns | 3.01× |
| `fstree.lookup_entry` | width-40000/hit | 1 | 103.945 us | 61.578 us | 1.69× |
| `fstree.lookup_entry` | width-40000/miss | 1 | 639.5 ns | 215.0 ns | 2.97× |
| `fstree.lookup_entry` | width-4096/hit | 1 | 79.856 us | 47.986 us | 1.66× |
| `fstree.lookup_entry` | width-4096/miss | 1 | 4.165 us | 2.132 us | 1.95× |
| `fstree.reachable_keys` | deep | auto | 8.083 us | 62.250 us | 0.13× |
| `fstree.reachable_keys` | file-corpus | auto | 812.5 ns | 5.016 us | 0.16× |
| `fstree.reachable_keys` | width-16 | auto | 1.824 us | 12.529 us | 0.15× |
| `fstree.reachable_keys` | width-256 | auto | 552.1 ns | 1.270 us | 0.43× |
| `fstree.reachable_keys` | width-40000 | auto | 418.4 ns | 410.6 ns | 1.02× |
| `fstree.reachable_keys` | width-4096 | auto | 429.2 ns | 484.0 ns | 0.89× |
| `fstree.resolve_entry` | depth-1 | 1 | 6.215 us | 3.449 us | 1.80× |
| `fstree.resolve_entry` | depth-12 | 1 | 15.895 us | 8.117 us | 1.96× |
| `fstree.resolve_entry` | depth-24 | 1 | 26.539 us | 13.551 us | 1.96× |
| `fstree.resolve_entry` | depth-4 | 1 | 8.918 us | 4.719 us | 1.89× |
| `fstree.resolve_path` | depth-1 | 1 | 906.2 ns | 472.7 ns | 1.92× |
| `fstree.resolve_path` | depth-12 | 1 | 10.160 us | 5.211 us | 1.95× |
| `fstree.resolve_path` | depth-24 | 1 | 20.395 us | 10.320 us | 1.98× |
| `fstree.resolve_path` | depth-4 | 1 | 3.371 us | 1.762 us | 1.91× |
| `fstree.write_content` | file-corpus | 1 | 265.6 ns | 25.500 us | 0.01× |
| `gc.close` | idle | 8 | 3.000 us | 2.000 us | 1.50× |
| `gc.open` | populated | 8 | 48.000 us | 32.000 us | 1.50× |
| `gc.prepare_ref` | missing-root | 8 | 3.422 us | 187.5 ns | 18.25× |
| `gc.prepare_ref` | tree-root/abort | 8 | 7.423 ms | 8.627 ms | 0.86× |
| `gc.prepare_ref` | tree-root/commit | 8 | 7.463 ms | 8.019 ms | 0.93× |
| `gc.release_ref` | batch | 8 | 8.3 ns | 1.5 ns | 5.67× |
| `gc.run` | nothing-to-reclaim | 8 | 3.783 ms | 1.598 ms | 2.37× |
| `gc.run` | reclaimable-packs | 8 | 12.401 ms | 10.192 ms | 1.22× |
| `gc.status` | mark+score | 8 | 5.427 ms | 3.787 ms | 1.43× |
| `gc.why` | live-root | 8 | 60.000 us | 7.000 us | 8.57× |
| `gc.wipe` | store-reset | 8 | 6.587 ms | 4.880 ms | 1.35× |
| `inbox.close` | drained | 8 | 34.687 ms | 40.103 ms | 0.86× |
| `inbox.commit` | packs/duplicate | 8 | 23.042 us | 20.625 us | 1.12× |
| `inbox.discard` | packs | 8 | 18.688 us | 17.854 us | 1.05× |
| `inbox.drain` | packs/new | 8 | 1.552 ms | 1.516 ms | 1.02× |
| `inbox.open` | empty | 8 | 66.000 us | 173.000 us | 0.38× |
| `inbox.open` | sweeps-staged-tmp-files | 8 | 18.062 us | 26.188 us | 0.69× |
| `inbox.stage` | packs | 8 | 89.396 us | 89.583 us | 1.00× |
| `ingest.dir` | single-file/jobs-1 | 1 | 35.487 ms | 21.297 ms | 1.67× |
| `ingest.dir` | tree/fresh-store/jobs-1 | 1 | 264.432 us | 37.472 us | 7.06× |
| `ingest.dir` | tree/fresh-store/jobs-N | 8 | 611.865 us | 53.720 us | 11.39× |
| `ingest.dir` | tree/incremental-change/jobs-1 | 1 | 208.947 us | 19.847 us | 10.53× |
| `ingest.dir` | tree/incremental-change/jobs-N | 8 | 567.541 us | 37.602 us | 15.09× |
| `ingest.dir` | tree/unchanged-repeat/jobs-1 | 1 | 208.718 us | 19.495 us | 10.71× |
| `ingest.dir` | tree/unchanged-repeat/jobs-N | 8 | 557.042 us | 38.028 us | 14.65× |
| `ingest.objects` | tree/jobs-1 | 1 | 203.751 us | 17.275 us | 11.79× |
| `ingest.objects` | tree/jobs-N | 8 | 556.739 us | 35.613 us | 15.63× |
| `ingest.scan` | filtered/jobs-1 | 1 | 2.960 us | 4.860 us | 0.61× |
| `ingest.scan` | filtered/jobs-N | 8 | 2.817 us | 18.255 us | 0.15× |
| `ingest.scan` | unfiltered/jobs-1 | 1 | 2.287 us | 3.032 us | 0.75× |
| `key.accessors` | type+length+length_size+hash | 1 | 13.0 ns | 8.4 ns | 1.55× |
| `key.new` | 1MiB-duplicate | 1 | 190.625 us | 132.375 us | 1.44× |
| `key.new` | 1MiB-random | 1 | 192.375 us | 120.625 us | 1.59× |
| `key.new` | 1MiB-text | 1 | 192.125 us | 122.625 us | 1.57× |
| `key.new` | 4KiB-duplicate | 1 | 1.758 us | 1.067 us | 1.65× |
| `key.new` | 4KiB-random | 1 | 1.782 us | 1.067 us | 1.67× |
| `key.new` | 4KiB-text | 1 | 1.793 us | 1.068 us | 1.68× |
| `key.new` | empty | 1 | 107.0 ns | 77.5 ns | 1.38× |
| `key.new` | large-8MiB-duplicate | 1 | 1.518 ms | 946.000 us | 1.61× |
| `key.new` | large-8MiB-random | 1 | 1.540 ms | 949.000 us | 1.62× |
| `key.new` | large-8MiB-text | 1 | 1.527 ms | 951.000 us | 1.61× |
| `key.new` | tiny-64B-duplicate | 1 | 107.1 ns | 82.1 ns | 1.30× |
| `key.new` | tiny-64B-random | 1 | 107.1 ns | 81.5 ns | 1.31× |
| `key.new` | tiny-64B-text | 1 | 106.9 ns | 81.5 ns | 1.31× |
| `key.new_from_hash` | batch | 1 | 43.0 ns | 35.5 ns | 1.21× |
| `key.parse` | canonical | 1 | 33.5 ns | 25.0 ns | 1.34× |
| `key.parse` | malformed | 1 | 62.5 ns | 1.0 ns | 64.00× |
| `key.string` | hex | 1 | 89.5 ns | 513.0 ns | 0.17× |
| `key.type_string` | names | 1 | 15.0 ns | 14.5 ns | 1.03× |
| `key.validate` | canonical | 1 | 10.0 ns | 1.0 ns | 10.00× |
| `packstore.append_record` | pre-encoded+sync | 1 | 6.410 us | 7.262 us | 0.88× |
| `packstore.barrier` | begin+observe+abort | 1 | 53.5 ns | 49.2 ns | 1.09× |
| `packstore.close` | populated | 1 | 385.000 us | 270.000 us | 1.43× |
| `packstore.compact` | 90-percent-dead | 1 | 567.0 ns | 770.7 ns | 0.74× |
| `packstore.compact` | nothing-dead | 1 | 48.6 ns | 15.4 ns | 3.15× |
| `packstore.get` | hit | 1 | 7.090 us | 4.441 us | 1.60× |
| `packstore.get` | miss | 1 | 57.0 ns | 51.5 ns | 1.11× |
| `packstore.get_record` | hit | 1 | 368.5 ns | 216.5 ns | 1.70× |
| `packstore.has` | hit | 1 | 96.0 ns | 60.0 ns | 1.60× |
| `packstore.has` | miss | 1 | 53.0 ns | 37.5 ns | 1.41× |
| `packstore.has_outside` | sealed-segment | 1 | 86.0 ns | 40.5 ns | 2.12× |
| `packstore.liveness` | tenth-live | 1 | 728.000 us | 595.000 us | 1.22× |
| `packstore.mark_set_contains` | all-keys | 1 | 128.3 ns | 124.3 ns | 1.03× |
| `packstore.mark_set_mark` | all-keys | 1 | 70.8 ns | 63.3 ns | 1.12× |
| `packstore.missing` | half-present | 1 | 466.5 ns | 515.5 ns | 0.90× |
| `packstore.new_mark_set` | snapshot | 1 | 138.938 us | 59.688 us | 2.33× |
| `packstore.oldest_inflight_write` | idle | 1 | 15.1 ns | 3.4 ns | 4.43× |
| `packstore.open` | empty | 1 | 31.000 us | 11.000 us | 2.82× |
| `packstore.open` | populated-reopen | 1 | 1.154 ms | 1.404 ms | 0.82× |
| `packstore.put` | 1MiB-duplicate/sync-off | 1 | 464.500 us | 228.000 us | 2.04× |
| `packstore.put` | 1MiB-random/sync-off | 1 | 887.000 us | 440.000 us | 2.02× |
| `packstore.put` | 1MiB-text/sync-off | 1 | 3.608 ms | 1.867 ms | 1.93× |
| `packstore.put` | 4KiB-duplicate/sync-off | 1 | 355.5 ns | 242.2 ns | 1.47× |
| `packstore.put` | 4KiB-random/already-present | 1 | 117.2 ns | 78.1 ns | 1.50× |
| `packstore.put` | 4KiB-random/sync-off | 1 | 6.785 us | 7.113 us | 0.95× |
| `packstore.put` | 4KiB-random/sync-on | 1 | 22.480 us | 22.336 us | 1.01× |
| `packstore.put` | 4KiB-text/sync-off | 1 | 26.465 us | 15.699 us | 1.69× |
| `packstore.put` | empty/sync-off | 1 | 89.0 ns | 77.1 ns | 1.15× |
| `packstore.put` | large-8MiB-duplicate/sync-off | 1 | 3.245 ms | 1.524 ms | 2.13× |
| `packstore.put` | large-8MiB-random/sync-off | 1 | 7.409 ms | 2.972 ms | 2.49× |
| `packstore.put` | large-8MiB-text/sync-off | 1 | 26.588 ms | 14.246 ms | 1.87× |
| `packstore.put` | tiny-64B-duplicate/sync-off | 1 | 86.1 ns | 77.2 ns | 1.11× |
| `packstore.put` | tiny-64B-random/sync-off | 1 | 1.693 us | 1.636 us | 1.03× |
| `packstore.put` | tiny-64B-text/sync-off | 1 | 2.305 us | 2.195 us | 1.05× |
| `packstore.put_verified` | already-intact | 1 | 87.572 us | 53.084 us | 1.65× |
| `packstore.record` | by-location | 1 | 465.0 ns | 594.0 ns | 0.78× |
| `packstore.remove` | one-sealed-segment | 1 | 824.000 us | 625.000 us | 1.32× |
| `packstore.scan_index` | all-segments | 1 | 29.4 ns | 23.9 ns | 1.23× |
| `packstore.segments` | list | 1 | 7.250 us | 6.094 us | 1.19× |
| `packstore.sort_by_location` | scattered | 1 | 193.5 ns | 91.0 ns | 2.13× |
| `packstore.stored_size` | hit | 1 | 97.5 ns | 53.5 ns | 1.82× |
| `packstore.verify` | full-scrub | 1 | 9.274 us | 5.876 us | 1.58× |
| `packstore.wipe` | populated | 1 | 6.222 ms | 3.827 ms | 1.63× |
| `packstore.write_batch` | mixed-objects | 1 | 32.018 us | 20.750 us | 1.54× |
| `packstore.write_parallel` | duplicate-stream/writers-N | 8 | 573.0 ns | 3.128 us | 0.18× |
| `packstore.write_parallel` | mixed-objects/writers-1/verify-false | 1 | 33.507 us | 22.658 us | 1.48× |
| `packstore.write_parallel` | mixed-objects/writers-1/verify-true | 1 | 35.946 us | 24.269 us | 1.48× |
| `packstore.write_parallel` | mixed-objects/writers-N/verify-false | 8 | 35.329 us | 25.847 us | 1.37× |
| `packstore.write_parallel` | mixed-objects/writers-N/verify-true | 8 | 37.911 us | 27.535 us | 1.38× |
| `reference.decode` | signed | 1 | 878.9 ns | 433.6 ns | 2.03× |
| `reference.encode` | signed | 1 | 293.0 ns | 168.0 ns | 1.74× |
| `reference.signature_payload` | signed | 1 | 273.4 ns | 175.8 ns | 1.56× |
| `reference.validate_name` | valid | 1 | 82.0 ns | 31.2 ns | 2.62× |
| `reference.validate_user` | valid | 1 | 101.6 ns | 93.8 ns | 1.08× |
| `refstore.all` | records | 1 | 110.6 ns | 161.2 ns | 0.69× |
| `refstore.close` | populated | 1 | 79.000 us | 706.000 us | 0.11× |
| `refstore.delete` | records | 1 | 1.713 us | 23.496 us | 0.07× |
| `refstore.get` | hit | 1 | 1.024 us | 556.5 ns | 1.84× |
| `refstore.get` | miss | 1 | 211.5 ns | 344.0 ns | 0.61× |
| `refstore.open` | empty | 1 | 405.000 us | 982.000 us | 0.41× |
| `refstore.open` | populated-reopen | 1 | 2.118 ms | 355.000 us | 5.97× |
| `refstore.put` | records/sync-false | 1 | 607.0 ns | 25.241 us | 0.02× |
| `refstore.put` | records/sync-true | 1 | 11.402 us | 25.289 us | 0.45× |
| `refstore.put_batch` | records/sync-false | 1 | 116.8 ns | 861.2 ns | 0.14× |
| `refstore.wipe` | records | 1 | 1.209 ms | 16.288 ms | 0.07× |
| `tarexport.write` | fixture-tree | 1 | 2.356 us | 2.259 us | 1.04× |
| `tarextract.extract` | fixture-tree | 1 | 39.714 us | 31.969 us | 1.24× |

## Encoded sizes (per core, never divided)

The two cores compress record payloads with different encoders —
`klauspost/compress` in Go, libzstd in Rust — so the same input legitimately
becomes a different number of bytes. Those sizes are side by side here and
are never used as a shared denominator; the throughput figures above divide
by the *input*, which is byte-identical.

| Operation | Workload | Items | Input | Go encoded | Rust encoded | Go/Rust |
|---|---|---:|---:|---:|---:|---:|
| `amberpack.encode_record` | 1MiB-duplicate | 4 | 4.00 MiB | 4.00 MiB | 4.00 MiB | 1.000× |
| `amberpack.encode_record` | 1MiB-random | 4 | 4.00 MiB | 4.00 MiB | 4.00 MiB | 1.000× |
| `amberpack.encode_record` | 1MiB-text | 4 | 4.00 MiB | 786.29 KiB | 780.43 KiB | 1.008× |
| `amberpack.encode_record` | 4KiB-duplicate | 1,024 | 4.00 MiB | 4.04 MiB | 4.04 MiB | 1.000× |
| `amberpack.encode_record` | 4KiB-random | 1,024 | 4.00 MiB | 4.04 MiB | 4.04 MiB | 1.000× |
| `amberpack.encode_record` | 4KiB-text | 1,024 | 4.00 MiB | 1.18 MiB | 1.18 MiB | 0.997× |
| `amberpack.encode_record` | empty | 8,000 | 0 B | 359.38 KiB | 359.38 KiB | 1.000× |
| `amberpack.encode_record` | large-8MiB-duplicate | 2 | 16.00 MiB | 16.00 MiB | 16.00 MiB | 1.000× |
| `amberpack.encode_record` | large-8MiB-random | 1 | 8.00 MiB | 8.00 MiB | 8.00 MiB | 1.000× |
| `amberpack.encode_record` | large-8MiB-text | 1 | 8.00 MiB | 1.53 MiB | 1.52 MiB | 1.007× |
| `amberpack.encode_record` | tiny-64B-duplicate | 16,384 | 1.00 MiB | 1.72 MiB | 1.72 MiB | 1.000× |
| `amberpack.encode_record` | tiny-64B-random | 16,384 | 1.00 MiB | 1.72 MiB | 1.72 MiB | 1.000× |
| `amberpack.encode_record` | tiny-64B-text | 16,384 | 1.00 MiB | 1.72 MiB | 1.72 MiB | 1.001× |
| `amberpack.writer_add` | mixed-objects | 500 | 16.17 MiB | 9.60 MiB | 9.59 MiB | 1.002× |

## Shared wire inputs

A decoder measured against its own encoder's output is measured against a
different input in each core. These packs are produced once, before anything
is measured, and **both** drivers read **both** of them: every reader and
decoder case runs once per producer, over byte-identical input.

| Producer | Objects | Encoded size | SHA-256 |
|---|---:|---:|---|
| go | 500 | 9.60 MiB | `9b79ead214d8233959487849689abacf67a163046dd8162a5b1933ea695a2c94` |
| rust | 500 | 9.59 MiB | `eb95ff79d63a86adf3303d5678b9b5bc00a6d0334fc14490adc7728427706052` |

## Single-core operations

These have no counterpart in the other core, which declares them unsupported
with a reason. They carry a measurement from one side only and are never
compared. See `COVERAGE.md`.

| Core | Operation | Workload | n | ns/op | var |
|---|---|---|---:|---:|---:|
| go | `gc.begin_write` | gate-span | 7 | 34.6 ns | 1.0% |
| rust | `binaryfuse.contains` | store-keys/rust-only | 7 | 2.8 ns | 0.2% |
| rust | `binaryfuse.new` | store-keys/rust-only | 7 | 15.3 ns | 0.4% |
| rust | `binaryfuse.parse_section` | store-keys/rust-only | 7 | 1.919 us | 0.5% |
| rust | `binaryfuse.section_bytes` | store-keys/rust-only | 7 | 2.538 us | 3.1% |
| rust | `cbor.head_primitives` | append+read/rust-only | 7 | 5.1 ns | 0.0% |

## Runtime counters (not comparable)

The Go runtime accounts for heap allocation; the Rust driver deliberately
runs with an uninstrumented allocator, because counting allocations there
would have cost time inside the very intervals being measured. These numbers
are therefore **Go-only** and are never used in a comparison.

| Operation | Workload | Counter | Median |
|---|---|---|---:|
| `ingest.dir` | tree/fresh-store/jobs-N | go.heap_alloc_bytes | 11,343,505,496 |
| `ingest.dir` | tree/fresh-store/jobs-1 | go.heap_alloc_bytes | 11,342,957,784 |
| `ingest.dir` | tree/incremental-change/jobs-N | go.heap_alloc_bytes | 11,250,755,344 |
| `ingest.dir` | tree/incremental-change/jobs-1 | go.heap_alloc_bytes | 11,250,436,592 |
| `ingest.dir` | tree/unchanged-repeat/jobs-N | go.heap_alloc_bytes | 11,233,394,720 |
| `ingest.dir` | tree/unchanged-repeat/jobs-1 | go.heap_alloc_bytes | 11,233,087,000 |
| `ingest.objects` | tree/jobs-N | go.heap_alloc_bytes | 11,232,812,392 |
| `ingest.objects` | tree/jobs-1 | go.heap_alloc_bytes | 11,232,508,240 |
| `amberpack.reader_all` | truncated-stream/producer-rust | go.heap_alloc_bytes | 898,344,832 |
| `amberpack.reader_all` | truncated-stream/producer-go | go.heap_alloc_bytes | 898,325,424 |
| `fstree.encode_file_node` | children-65536 | go.heap_alloc_bytes | 386,402,936 |
| `fstree.decode_file_node` | children-65536 | go.heap_alloc_bytes | 369,101,824 |
| `packstore.verify` | full-scrub | go.heap_alloc_bytes | 256,131,800 |
| `tarextract.extract` | fixture-tree | go.heap_alloc_bytes | 195,419,048 |
| `fstree.lookup_entry` | width-40000/hit | go.heap_alloc_bytes | 153,632,960 |
| `packstore.write_batch` | mixed-objects | go.heap_alloc_bytes | 129,912,920 |
| `packstore.write_parallel` | mixed-objects/writers-N/verify-false | go.heap_alloc_bytes | 129,577,488 |
| `packstore.write_parallel` | mixed-objects/writers-N/verify-true | go.heap_alloc_bytes | 129,577,296 |
| `packstore.write_parallel` | mixed-objects/writers-1/verify-false | go.heap_alloc_bytes | 129,574,840 |
| `packstore.write_parallel` | mixed-objects/writers-1/verify-true | go.heap_alloc_bytes | 129,574,840 |
| `fstree.lookup_entry` | width-4096/hit | go.heap_alloc_bytes | 119,309,840 |
| `chunkers.split_bytes` | random/default-sizes | go.heap_alloc_bytes | 72,802,592 |
| `chunkers.split_bytes` | compressible/default-sizes | go.heap_alloc_bytes | 69,206,304 |
| `chunkers.split_bytes` | compressible/4-16-64KiB | go.heap_alloc_bytes | 67,240,224 |
| `fstree.lookup_entry` | width-256/hit | go.heap_alloc_bytes | 60,870,912 |

The full set (496 rows) is in `counters.csv`.

## Resident set

`getrusage` reports one number per process: the high-water resident set over
the **whole lifetime of the driver**, which only ever grows. It is not a
per-operation peak and it cannot be attributed to a case — a case measured
after a large fixture was built inherits that fixture's high-water mark. The
only honest reading is the one below: how much memory each driver had touched
by the end of its run.

* Go driver, end of run: 1,823,900 KiB
* Rust driver, end of run: 1,407,600 KiB

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

### amberpack-3

![amberpack-3](plots/amberpack-3.svg)

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
./run.sh --profile standard --seed 1592639710 --cpus 8
```

Raw data next to this file: `go-pass*.json`, `rust-pass*.json` (per-repetition
samples, one document per measurement pass), `samples.csv`, `summary.csv`,
`paired.csv`, `scaling.csv`, `checks.csv`, `counters.csv`, `encodings.csv`,
`unsupported.csv`, `report.json`.

## Scope limits

* Measurements are warm. Nothing here says anything about cold-cache behaviour.
* 7 repetitions on one host. The dispersion columns are the honest
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
