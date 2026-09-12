# Amber core operations: Go vs Rust, in process

This repository measures one thing: the two Amber-Store cores — the Go core
at `github.com/amber-store/core` and the Rust core at
`github.com/amber-store/core-rs` — against each other, **operation by
operation, through their library APIs**, with no process start inside any
measured interval.

Latest recorded results: [standard report](results/20260912T233829Z/standard/REPORT.md)
and [run details](results/README.md). Both profiles ran on a dedicated AMD Ryzen 9 7950X3D machine.

```sh
./run.sh                      # quick profile: correctness first, ~25 s
./run.sh --profile standard   # the measurement profile
./run.sh --profile both       # quick, then standard
./run.sh --out ./my-results   # choose where the report goes
```

Nothing has to be prepared first. Two small native drivers — one Go
(`core-ops/go/`), one Rust (`core-ops/rust/`) — link the corresponding core
as a library and call its exported operations directly. `run.sh` builds both,
proves they were linked against the revisions `flake.lock` pins, runs them
sequentially in a balanced order and writes a report.

| | |
|---|---|
| Paired operations | 100 |
| Paired operation/workload cases | 247 |
| Correctness checks | 183 Go, 184 Rust |
| Checks that assert the two cores produced identical output | 171 |
| Exported symbols accounted for | 198 Go, 279 Rust |
| Operations one core exports and the other does not | 1 Go-only, 5 Rust-only |
| Module groups | `key`, `cbor`, `binaryfuse`, `chunkers`, `fstree`, `amberignore`, `ingest`, `amberpack`, `packstore`, `reference`, `refstore`, `inbox`, `tar`, `gc` |

## What a workload is

A workload is a point in a grid, and the grid is recorded as numbers rather
than as a label. Every sample carries a `dims` object — object size, item
count, content kind, entry count, path depth, directory width, file count,
stored objects, worker count — and the report plots a dimension by sweeping
it with the others held fixed.

The swept dimensions are:

| Dimension | Points | Crossed with |
|---|---|---|
| Object size | empty, 64 B, 4 KiB, 1 MiB, 8 MiB | random, compressible and duplicate content |
| Directory width | 16, 256, 4096, 40 000 entries | hit and miss, for the lookups |
| Path depth | 1, 4, 12, 24 levels | — |
| File-index fan-out | 8, 128, 1024, 65 536 children | — |
| Node entry count | 8, 128, 1024 | plain, with extended attributes, and one entry rewritten |
| Workers | 1 and the profile's concurrent count | — |

The empty object is not crossed with the content kinds: a zero-length object
has no content to be random, compressible or duplicated, so the grid has one
`empty` point rather than three identical ones. Structured nodes and the
partial-change case — a 128-entry directory leaf with a single entry's
content key rewritten, which is the shape an incremental update produces —
are their own points, not points on the entry-count curve they would
otherwise distort.

These are **absolute** sizes and counts, identical in every profile. A
profile changes how many repetitions are measured and how large the corpora
and stores are; it does not change what a workload means. That is what lets
one coverage manifest describe every profile, and what lets a quick run's
scaling plot be read beside a standard one's.

## What each number is divided by

A rate is only as good as its denominator, and the denominators here are
exact counts taken outside every measured interval:

* `ingest.scan` walks directory entries and stats inodes; it never opens a
  file body. Its byte figure is labelled **logical bytes scanned** — the size
  of the tree it covered, not bandwidth.
* A filtered walk, an unfiltered walk and the changed successor tree cover
  different files and different bytes. Each is measured once at fixture-build
  time with the core's own scan and used as that case's denominator.
* `packstore.scan_index` covers every sealed segment rather than the first,
  because which object lands in which segment follows the compressed record
  sizes and the two cores' encoders do not agree on those.
* Encoded sizes are reported per core and never divided by one another. The
  throughput figures divide by the **input**, which is byte-identical.

## Correctness comes first

Every run builds its fixtures, then runs the whole correctness suite, and
only then measures anything. A single failed check stops the run before the
first timing is taken, and the report refuses to publish a comparison from a
document with a failed check.

The checks are not smoke tests. They assert content keys, round trips,
expected errors with their specific classification, paging invariants, ignore
semantics, retained data after garbage collection, and repair of injected
corruption. Where the two cores are specified to agree byte for byte, the
check records a **canonical digest** and the report requires the two cores'
digests to be equal — 171 of them, covering the fixture trees, every encoder,
the chunk boundary list, the ingest root key and the exported tar archive.

Two of them are whole-set comparisons rather than spot checks:

* **The restored tree.** The fixture tree is ingested, exported as a PAX
  archive from the resulting root, and extracted into an empty directory. The
  complete listing of what comes out — every path, its type, permissions,
  size, mtime, content digest and symlink target — is compared line for line
  against a listing the harness derives from the source tree and the
  fixture's own ignore rules. Nothing in that comparison comes from either
  core; two cores agreeing with each other would prove only that they agree.
* **What compaction kept.** Every object that was supposed to survive is read
  back and re-hashed, and the store is content-addressed, so a key that
  matches its own re-hash is a complete verification of the retained bytes.
  Everything the predicate called dead is shown to be gone.

Nothing destructive touches a shared fixture: compaction, removal, wipe and
repair each run against a fresh copy made outside the measured interval.

Every timed operation points at a check. `coverage/matrix.py check` fails if
one does not, because a timing with no evidence is a measurement of an
unknown operation.

## Fairness

* Both drivers get the same profile: same seed, same fixture sizes, same
  operation counts, same chunk sizes, same segment size, same durability
  settings, same repetition counts. The report compares **every** field of
  both configuration blocks, not a hand-written list.
* Fixtures are generated from the same SplitMix64 stream in both languages,
  down to file contents, permissions and modification times. The fixture
  digests are checks, so equality is proven per run rather than assumed.
* Both drivers are pinned to the same CPU set with `taskset` and run
  **sequentially**, never overlapping. The repetitions are split across two
  passes of opposite core order — Go first, then Rust first — so neither core
  is systematically measured on the colder machine.
* **Decoders are measured on byte-identical input.** The two cores' zstd
  encoders do not produce the same bytes, so a decoder measured against its
  own encoder's output would be measured against a different input on each
  side. Before anything is timed, both drivers write their encoding of the
  same object population into a shared directory; then both drivers read
  **both** packs, and every reader and decoder case runs once per producer,
  recording the producer, the content hash and the encoded size.
* Operations that take a worker count are measured at one worker and at the
  profile's concurrent count; operations that pick their own parallelism are
  marked `auto`, and the report requires both cores to have got the same
  number of workers.
* Short operations are batched — a few hundred to a few thousand calls per
  measured interval — and every case's latency is reported per core
  operation.

### What the measured interval contains

The core call, and a constant-time consumption of its output. Nothing else.

Every case returns an accumulator built from what its calls produced, and
that accumulator is recorded in the sample, so no compiler can decide the
work was dead. Consumption is deliberately cheap: a fixed-size output (a
32-byte key, a parsed record header) is folded whole, and a byte stream goes
through a never-inlined sink that touches its length and both ends. Folding
whole payloads inside the interval would turn an encode measurement into
encode plus a second hash.

The full-output digests that prove the two cores agree are computed by the
correctness suite, which runs to completion before the first timing is taken.
160 of the 247 shared cases additionally require the two cores' accumulators
to be equal, and the report refuses the run if they are not.

### Disclosed asymmetries

* The Go driver runs a full garbage collection before every measured
  repetition; the Rust driver has no collector to run. This is recorded in
  both documents as `forced_gc_before_rep` and stated in every report.
* `getrusage` reports one resident-set number per process: the high-water
  mark over the whole lifetime of the driver, which only ever grows. It is
  not a per-operation peak and cannot be attributed to a case. The report
  gives the end-of-run figure for each driver and says so.
* `refstore` is Pebble in Go and redb in Rust. The operation is the same; the
  storage engine is not, and a `refs/` directory written by one core is not
  readable by the other.
* Go runtime allocation counters have no Rust counterpart. They are reported
  in their own table and never used in a comparison.

## Provenance

Every recorded result carries both cores' full commit ids, their dirty state,
and the SHA-256 of each driver executable. The harness revision is recorded
separately, so a harness edit is never mistaken for a core change.

`run.sh` establishes this **before** it measures anything:

1. It reads the pinned revisions from `flake.lock`.
2. It requires `core-ops/rust/Cargo.toml` and `Cargo.lock` to name the same
   Rust revision — both the commit that was *requested* and the one after the
   `#` that cargo actually *resolved* — and it asks `cargo metadata` what the
   build it just did really used.
3. It compares **every file** of the resolved Go module against the flake's
   checkout of the pinned commit: assembly, cgo headers, embedded data,
   `go.mod` and `go.sum` all change what the driver links, so a `.go`-only
   comparison would not close the gap between a content hash and a commit.
   Any difference stops the run before a single measurement.
4. It writes `identity.json`, then runs.
5. Before the report is written — not after — it re-hashes both executables
   and the benchmark's own sources, and checks that each raw sample document
   records the revision, dirty state and executable hash the identity
   document recorded. A report that looked valid and was then contradicted by
   a later check would be a publishable artefact of an invalid run, so there
   is no window in which one exists.

With `AMBER_GO_REPO` or `AMBER_RUST_REPO` pointing at a local checkout, the
run records that checkout's revision and dirty flag instead, and marks itself
`pin_verified: false`.

## Profiles

| | `quick` | `standard` |
|---|---|---|
| Purpose | correctness, fast | measurement |
| Repetitions | 1 (after 0 warm-ups) | 7 (after 2 warm-ups) |
| Corpora | 4 MiB | 64 MiB |
| Payload grid, per point | 1 MiB | 8 MiB |
| Fixture tree | 120 files | 1200 files |
| Store fixture | 1 500 objects, 2 MiB segments | 30 000 objects, 16 MiB segments |
| References / inbox packs | 200 / 8 | 5 000 / 48 |
| Concurrent worker count | 4 | 8 |

Both profiles run every correctness check and measure every workload; the
swept dimensions are identical. `quick` takes about half a minute for both
cores. With one repetition there is no dispersion and no test statistic, and
the report says so in place of the confidence interval, the variation column
and the *p* column.

## Output

```
results/<timestamp>/
  identity.json            source and executable identity, recorded pre-run
  quick/ | standard/
    go-pass*.json          raw per-repetition samples, one per measurement pass
    rust-pass*.json
    REPORT.md              GitHub-readable tables
    report.json            everything the Markdown says, machine-readable
    samples.csv            every raw sample, with its dimensions
    summary.csv            per-case dispersion and every rate
    paired.csv             the paired comparison with the test statistics
    scaling.csv            the scaling sweeps
    checks.csv             every correctness check and its digest
    counters.csv           Go runtime counters (never compared)
    encodings.csv          each core's encoded sizes (never divided)
    unsupported.csv        operations one core does not export
    plots/*.svg            one chart per group page and per scaling sweep
```

The sample schema is documented in [core-ops/SCHEMA.md](core-ops/SCHEMA.md);
what is measured and why is in [core-ops/COVERAGE.md](core-ops/COVERAGE.md).

## Statistics

`core-ops/report/report.py` recomputes everything from the raw samples:
median, the quartiles, the median absolute deviation, relative variation,
per-operation CPU time, and rates in the unit the operation is actually in —
operations per second, entries per second, objects per second, payload bytes
per second, encoded bytes per second. Paired rows carry the ratio of medians
with a 95 % percentile-bootstrap interval and a two-sided Mann-Whitney U test
— exact for the small, tie-free samples a profile produces,
normal-approximated with a tie correction otherwise. The bootstrap uses the
same SplitMix64 generator the fixtures do, so a report is reproducible from
its own raw data.

Seven repetitions on one host is what a standard run is. The variation column
is the honest bound on what that supports: differences of the order of the
variation are not differences, and an interval that spans 1 is not evidence.
There is no overall ranking, and there is no intention to produce one.

## What refuses to publish

`core-ops/report/validate.py` decides whether a comparison may exist at all.
Each of its rules is there because a run that broke it would still have
produced a plausible-looking table:

* both documents carry the schema and declare the core they claim to be;
* every field of both configuration blocks is identical;
* both cores have a 40-character revision and a 64-character executable hash,
  **and each raw document records the same ones as the identity document**;
* the two runs saw the same host, filesystem, extended-attribute support and
  automatic parallelism;
* neither check list is empty, no check id appears twice, no check failed;
* every check marked comparable exists on both sides with an equal, non-empty
  digest;
* neither sample list is empty; every sample has a finite, positive time and
  operation count, and every byte count says what it counts;
* every case has exactly the profile's repetitions, each index once, and
  every repetition of a case describes the same work as its siblings;
* every shared case agrees on threads, operation count, byte count and every
  workload dimension — a shared label over different work is refused;
* **a shared case present on one side only is a broken pair, not an operation
  one core does not export**; a single-core case must belong to an operation
  the other core explicitly declares unsupported, with a reason;
* every stable case produced the same output in every repetition, and every
  case that requires it produced the same output in both cores;
* both cores decoded the same wire bytes and encoded the same input;
* the measured set is exactly the coverage matrix's — a workload that
  disappeared and one that appeared without being written down are both
  failures.

`core-ops/report/test_report.py` has a test for each of those refusals, and
`core-ops/report/probe.py` runs the same probes against a real run's
documents:

```sh
python3 core-ops/report/probe.py results/<run>/quick
```

## Layout

```
run.sh                    the one command
core-ops/
  go/                     the Go driver (module, pinned by go.mod/go.sum)
  rust/                   the Rust driver (crate, pinned by Cargo.lock)
  report/report.py        statistics, Markdown, CSV, charts
  report/validate.py      what may be compared, and what may not
  report/stats.py         the statistics, with their own tests
  report/test_report.py   tests for the statistics and every refusal
  report/probe.py         the same refusals, against a real run
  coverage/               API extraction, the matrix and its checker
  COVERAGE.md             generated: every exported symbol, accounted for
  SCHEMA.md               the shared raw-sample schema
python/amber_bench_plot.py  the chart renderer; recomputes nothing
```

## Scope limits

* Measurements are warm. Caches are never dropped, and no claim about
  cold-cache behaviour appears anywhere.
* One host, one CPU set, one architecture.
* zstd-compressed record payloads come from `klauspost/compress` in Go and
  libzstd in Rust, so records, segment bodies and wire packs are
  interoperable but not byte-identical. Which sealed segment an object lands
  in follows the compressed sizes, so per-segment counts differ between the
  cores by construction.
* `packstore.ErrUnknownSegment` is the one error sentinel with no check:
  reaching it requires a segment id that was never sealed, and the benchmark
  has no operation that produces one.
