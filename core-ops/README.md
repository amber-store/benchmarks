# Amber core operations: Go vs Rust, in process

This is a second, separate benchmark in this repository. The suite at the
repository root (`../run.sh`) compares whole systems through their command
lines — Amber against git, restic, Nix and a plain filesystem CAS. This one
compares the **two Amber cores against each other, operation by operation,
through their library APIs**, with no process start inside any measured
interval.

The two benchmarks share nothing but the chart renderer. Their results live
in different directories (`../results/` and `../core-ops-results/`) and are
never mixed: a native library call and a CLI invocation are not the same
measurement, and putting them in one table would invite exactly the
comparison that makes no sense.

```sh
./core-ops/run.sh                      # quick profile: correctness first
./core-ops/run.sh --profile standard   # the measurement profile
./core-ops/run.sh --profile both       # quick, then standard
```

## What it measures

Two small native drivers — one Go (`go/`), one Rust (`rust/`) — link the
corresponding core as a library and call its exported operations directly.
Each driver builds its fixtures once, runs a registry of cases, and writes one
JSON document of raw per-repetition samples. `report/report.py` pairs the two
documents by `(operation, workload)` and recomputes every published number
from those samples.

Coverage is not a matter of opinion: `coverage/` extracts every exported
symbol from both pinned cores and `COVERAGE.md` maps each one to an operation,
a check, or a precise reason it is not an operation. The mapping is proven
total by `coverage/matrix.py check`, which CI runs.

| | |
|---|---|
| Paired operations | 98 |
| Exported symbols accounted for | 198 Go, 237 Rust |
| Operations exported by one core only | 2 Go-only, 5 Rust-only |
| Module groups | `key`, `cbor`, `binaryfuse`, `chunkers`, `fstree`, `amberignore`, `ingest`, `amberpack`, `packstore`, `reference`, `refstore`, `inbox`, `tar`, `gc` |

## Correctness comes first

Every run builds its fixtures, then runs the whole correctness suite, and only
then measures anything. A single failed check stops the run before the first
timing is taken, and `report.py` refuses to publish a comparison from a
document with a failed check.

The checks are not smoke tests. They assert content keys, round trips,
expected errors with their specific classification, paging invariants, ignore
semantics, retained data after garbage collection, and repair of injected
corruption. Where the two cores are specified to agree byte for byte, the
check records a **canonical digest** and the report requires the two cores'
digests to be equal — 150 of them, covering the fixture trees, every encoder,
the chunk boundary list, the ingest root key and the exported tar archive.
Where they are only specified to interoperate (zstd-compressed record
payloads), the digest is recorded as a within-core anchor and explicitly not
compared.

Nothing destructive touches a shared fixture: compaction, removal, wipe and
repair each run against a fresh copy made outside the measured interval.

## Fairness

* Both drivers get the same profile: same seed, same fixture sizes, same
  operation counts, same chunk sizes, same segment size, same durability
  settings, same repetition counts.
* Fixtures are generated from the same SplitMix64 stream in both languages,
  down to file contents, permissions and modification times. The fixture
  digests are checks, so equality is proven per run rather than assumed.
* Both drivers are pinned to the same CPU set with `taskset` and run
  **sequentially**. Operations that take a worker count are measured at one
  worker and at the profile's concurrent count; operations that pick their own
  parallelism are marked `auto` and see the same CPU set on both sides.
* Short operations are batched — a few hundred to a few thousand calls per
  measured interval — and every case accumulates a value that is printed at
  the end, so no compiler can elide the work.
* Fixture construction, store copies, and process start-up are all outside the
  measured interval.

## Provenance

Every recorded result carries both cores' full commit ids, their dirty state,
and the SHA-256 of each driver executable. The harness revision is recorded
separately, so a harness edit is never mistaken for a core change.

`run.sh` establishes this **before** it measures anything:

1. It reads the pinned revisions from `../flake.lock`.
2. It requires `rust/Cargo.toml` and `rust/Cargo.lock` to name the same Rust
   revision, and refuses to run otherwise.
3. It diffs the Go module the driver actually resolved against the flake's
   checkout of the pinned commit — a module hash pins content, the flake input
   pins the commit, and only the diff closes the gap.
4. It writes `identity.json`, then runs.
5. Afterwards it re-hashes both executables and the benchmark's own sources
   and fails if either changed while the run was in flight.

With `AMBER_GO_REPO` or `AMBER_RUST_REPO` pointing at a local checkout, the
run records that checkout's revision and dirty flag instead, and marks itself
`pin_verified: false`.

## Profiles

| | `quick` | `standard` |
|---|---|---|
| Purpose | correctness, fast | measurement |
| Repetitions | 1 (after 0 warm-ups) | 7 (after 2 warm-ups) |
| Corpora | 4 MiB | 64 MiB |
| Fixture tree | 120 files, 200 wide, depth 8 | 1200 files, 4000 wide, depth 24 |
| Synthetic directory | 2 000 entries | 40 000 entries |
| Store fixture | 1 500 objects, 2 MiB segments | 30 000 objects, 16 MiB segments |
| References / inbox packs | 200 / 8 | 5 000 / 48 |
| Concurrent worker count | 4 | 8 |

The `quick` profile runs every correctness check — the same ones — and takes
about half a minute for both cores. Its timings exist so the plumbing is
exercised; with one repetition there is no dispersion and no test statistic,
and the report says so in place of the confidence interval and *p* column.

## Output

```
core-ops-results/<timestamp>/
  identity.json            source and executable identity, recorded pre-run
  quick/ | standard/
    go.json  rust.json     raw per-repetition samples (the shared schema)
    REPORT.md              GitHub-readable tables
    report.json            everything the Markdown says, machine-readable
    samples.csv            every raw sample
    summary.csv            per-case dispersion
    paired.csv             the paired comparison with the test statistics
    checks.csv             every correctness check and its digest
    counters.csv           Go runtime counters (never compared)
    unsupported.csv        operations one core does not export
    plots/<group>.svg      one chart per module group, plus its spec as JSON
```

The sample schema is documented in [SCHEMA.md](SCHEMA.md).

## Statistics

`report/report.py` recomputes everything from the raw samples: median, the
quartiles, the median absolute deviation, per-operation CPU time, and
throughput where a byte count is meaningful. Paired rows carry the ratio of
medians with a 95 % percentile-bootstrap interval and a two-sided
Mann-Whitney U test — exact for the small, tie-free samples a profile
produces, normal-approximated with a tie correction otherwise. The bootstrap
uses the same SplitMix64 generator the fixtures do, so a report is
reproducible from its own raw data.

There is no overall ranking, and there is no intention to produce one.

## Scope limits

* Measurements are warm. Caches are never dropped, and no claim about
  cold-cache behaviour appears anywhere.
* `refstore` is Pebble in Go and redb in Rust. The operation is the same; the
  storage engine is not, and a `refs/` directory written by one core is not
  readable by the other.
* zstd-compressed record payloads come from `klauspost/compress` in Go and
  libzstd in Rust, so records, segment bodies and wire packs are interoperable
  but not byte-identical. Which sealed segment an object lands in follows the
  compressed sizes, so per-segment counts differ between the cores by
  construction.
* Go runtime allocation counters have no Rust counterpart. They are reported
  in their own table and never used in a comparison; the Rust driver
  deliberately runs an uninstrumented allocator, because counting there would
  cost time inside the intervals being measured.
* `packstore.ErrUnknownSegment` is the one error sentinel with no check:
  reaching it requires a segment id that was never sealed, and the benchmark
  has no operation that produces one.

## Layout

```
core-ops/
  run.sh                  the one command
  go/                     the Go driver (module, pinned by go.mod/go.sum)
  rust/                   the Rust driver (crate, pinned by Cargo.lock)
  report/report.py        validation, statistics, Markdown, CSV, charts
  report/stats.py         the statistics, with their own tests
  report/test_report.py   tests for the statistics and the validity gate
  coverage/               API extraction, the matrix and its checker
  COVERAGE.md             generated: every exported symbol, accounted for
  SCHEMA.md               the shared raw-sample schema
```
