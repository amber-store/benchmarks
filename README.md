# Amber-Store comparison benchmarks

A reproducible comparison of the two Amber-Store cores — the
[Rust core](https://github.com/amber-store/core-rs) and the
[Go core](https://github.com/amber-store/core) — against the systems they are
actually proposed to replace: **Git object storage**, a **Nix store**, and a
**backup repository** (restic). Each of those three groups is measured both
locally and pushing to **object storage**, and an uncompressed whole-file
SHA-256 store is included as a control.

Everything is driven through the command line each system ships, as a child
process, so the elapsed-time, CPU-time and peak-RSS columns mean the same
thing in every row.

```
./run.sh                          # smoke profile, every backend
./run.sh --profile standard       # bigger
./run.sh --out ./my-results       # choose where the report goes
```

That one command needs nothing prepared first, and nothing checked out
beside this repository. It enters this directory's Nix development shell, so
`git`, `restic`, `nix`, `garage` and `mc` all come from `flake.lock`; builds
**both** cores from the source revisions pinned as the flake's
`amber-rust-src` and `amber-go-src` inputs; builds the harness with a locked
Cargo resolution; and writes a report. Every executable it measured is
recorded in that report by path, reported version, SHA-256 of the executable
file, and the source revision it was built from.

Measurements that have been recorded and committed live in
**[`results/`](results/README.md)** — Markdown, static SVG charts and the
JSON/CSV behind them, readable in a browser with nothing installed.

---

## Contents

* [What is compared](#what-is-compared)
* [Reading a report](#reading-a-report)
* [Methodology](#methodology)
* [Fairness: what is not comparable](#fairness-what-is-not-comparable)
* [Commands and options](#commands-and-options)
* [Profiles and scaling](#profiles-and-scaling)
* [Object storage](#object-storage)
* [Correctness checks](#correctness-checks)
* [Recording results](#recording-results)
* [Extending the suite](#extending-the-suite)
* [Continuous integration](#continuous-integration)
* [The older micro-benchmarks](#the-older-micro-benchmarks)

---

## What is compared

Five scenario groups. A comparison is only ever made *within* a scenario,
because that is the only level at which two numbers describe the same work.

| group    | scenario            | backends                                       | what it asks |
|----------|---------------------|------------------------------------------------|--------------|
| `git`    | `git/history`        | `git`, `amber-rust`, `amber-go`                | retain a branching, tagged source history and hand every version back |
| `nix`    | `nix/closure`        | `nix`, `amber-rust`, `amber-go`                | hold a closure of store paths with references, symlinks and executables |
| `backup` | `backup/retention`   | `restic`, `amber-rust`, `amber-go`             | snapshot a changing tree, then apply retention |
| `tree`   | `tree/lifecycle`     | `amber-rust`, `amber-go`, `git`, `restic`, `fs-sha256` | the generic filesystem-tree workload, with the SHA-256 control |
| `blob`   | `blob/backup-corpus`, `blob/source-history`, `blob/nix-closure` | `restic-s3`, `git-bundle-s3`, `nix-binary-cache`, `amber-rust-s3`, `amber-go-s3` | publish the same data to a real S3 service and fetch it back |

### The local operations

Fresh ingestion, repeated (unchanged) ingestion, changed ingestion, listing,
streaming every byte back, materialising a tree, dropping a reference,
garbage collection — plus, outside the measured window, the backend's own
integrity check and a post-collection restore of everything that survived.

### The object-storage operations

Every backend in a `blob` scenario runs the same six measured operations in
the same order, against a real Garage instance over real HTTP:

| operation | what it does |
|---|---|
| `push_initial` | publish generation 0 |
| `push_noop` | publish it again, unchanged |
| `pull_fresh` | deliver everything published so far to a client that has never seen the store |
| `push_incremental` | publish generation 1 |
| `pull_incremental` | bring that same client up to date |
| `retention_cleanup` | drop generation 0 and propagate the deletion |

Each pull records **what it delivered** (`delivered_references`,
`delivered_logical_bytes`) next to what it cost, and every delivered
reference is then verified at the destination. That is deliberate: a backend
that downloads a whole repository and a backend that restores a single
snapshot have not done the same work, and a table that hid the difference
would be misleading even with correct timings.

In `blob/source-history` this is what decides what the Amber cores are given
to publish. A Git bundle carries every commit, branch and tag in the range it
covers, so the cores are handed **every retained version** in the same range
— the first three quarters of the history initially, the rest incrementally —
and each pull's `delivered_references` counter lets a reader confirm it. What
the two sides *record* about a version is still not the same thing, and the
report says so: Git keeps commit identity, authorship, parentage and branch
topology and stores no mtimes, no permission bits beyond the executable one
and no empty directories, while an Amber reference is a name for a tree root
and keeps all of those.

`retention_cleanup` is the one operation here where the backends are **not**
asked for the same thing. Consolidating Git bundles drops no version;
forgetting a restic snapshot, expiring a narinfo and dropping an Amber
reference each drop one. Every backend's row therefore carries
`retained_references` and a sentence saying what it kept, every member of
that set is verified afterwards, and the timings are not offered as a
ranking.

---

## Reading a report

Six artefacts land in `--out`:

| file | what it is |
|---|---|
| `REPORT.md` | the readable comparison, by scenario |
| `report.json` | everything, including every raw sample, every check and every failure |
| `samples.csv` | one row per operation per repetition, with `rep_valid` |
| `summary.csv` | the aggregated statistics, with `run_valid` on every row |
| `counters.csv` | every integer counter (S3 requests by method, bytes by direction, store component sizes, delivered references…) |
| `verifications.csv` | one row per correctness check |

Four rules hold everywhere in them:

1. **An empty cell is never a zero.** An operation a backend cannot perform
   is `unsupported` with a written reason; an operation that failed is
   `failed` with the error. Neither becomes a number.
2. **Only valid repetitions are aggregated.** If a repetition's restored
   bytes did not match their manifest, its timings are kept as raw samples
   and counted in `excluded_invalid_samples`, but they never enter a median:
   a fast operation that produced the wrong result is not a measurement of
   anything.
3. **An invalid run says so.** Any failure — including a *cross-backend*
   check such as the two cores disagreeing about a root key — makes
   `validity.valid` false in `report.json`, `run_valid` false in
   `summary.csv`, puts a banner at the top of `REPORT.md`, and makes the
   command exit non-zero.
4. **Setup and verification are never part of a measured number.**
   Generating the corpus, materialising a working tree and re-hashing a
   restored tree are timed under their own phase, for transparency, and
   excluded from every comparison.

`REPORT.md` deliberately has no overall score and no single winner. There is
no defensible way to rank a plaintext tree store against an encrypted backup
repository in one column.

---

## Methodology

**One corpus, shared by everyone.** A single deterministic generator writes
the corpus once per run; every backend is pointed at the same directories.
Content is a pure function of `(--seed, label)`, and timestamps are derived
from a fixed base rather than from the clock — both cores hash mtimes into
directory objects, so a corpus with wall-clock timestamps would produce a
different root key on every run. The corpus contains, separately named so a
report can say *where* a backend won:

| subtree | what it exercises |
|---|---|
| `tiny/` | per-object overhead: hundreds to tens of thousands of 0–4 KiB files |
| `random/` | raw throughput: large incompressible files |
| `compressible/` | the compressor: large text-like files |
| `duplicates/` | whole-file dedup: byte-identical copies |
| `shifted/` | content-defined chunking: bytes inserted near the front, so every later boundary moves |
| `mixed/` | tree metadata: deep directories, symlinks (including a dangling one), executables, odd permission bits, empty directories |
| `added/`, deletions, rewrites | snapshot churn between generations |

The Git group uses a second generator: a branching, tagged history with
hundreds of small source files (a handful rewritten per commit), a large
never-changing `vendor/` subtree, binary assets — one rewritten wholesale
part-way through — a real two-parent merge, and one side branch left
unmerged. Commits are made with fixed identities and fixed timestamps, so
commit ids are a function of the seed.

The Nix group uses a fixture closure built by this flake (`nix/fixtures.nix`):
shared leaf paths, mid-level paths with real references, executables,
relative and absolute store symlinks, a dangling symlink, and two
generations that share most of their closure.

**Independent verification.** Nothing is checked against the generator's
intentions. A manifest is produced by walking the tree on disk and hashing
what is there, and the same routine produces the manifest of a restored
tree; a comparison is therefore between two independent observations. Any
missing, extra or differing entry fails the run. For the Nix group the
harness additionally recomputes each restored path's NAR hash with `nix hash
path` and compares it with the hash the *source* Nix store recorded — which
needs no Nix support in either Amber core.

**Isolation.** Every (scenario, backend, repetition) triple gets a store of
its own, created before it and destroyed after it. Nix runs against isolated
stores (`nix --store <dir>`) with every host substituter switched off, so a
measurement can never be served from a cache and a missing object fails
instead of being fetched. restic's cache is pinned per client. Git never
reads the host's global or system configuration.

A Nix store directory is not the whole of a client's state, so it is not the
whole of the isolation either: Nix also keeps a per-user **narinfo cache**,
and a binary-cache query is answered out of it while the entry is inside its
TTL. A brand new store in the same environment is therefore not a new client.
Every client that is supposed to be cold — the one that substitutes from the
local binary cache, the one that pulls from S3 — gets a cache directory of
its own (`XDG_CACHE_HOME`) as well as an empty store, and then keeps it, so
the incremental pull that follows is a real incremental pull. The probe that
asks whether something is *really gone* from a cache gets a private cache
directory **and** both narinfo TTLs set to zero, because otherwise it would
happily fetch a NAR whose narinfo was deleted minutes earlier and report the
deletion as ineffective — which is exactly what an early run of this suite
observed.

**Randomised order.** Within each repetition the (scenario, backend) pairs
run in an order shuffled from `--order-seed`, which is recorded, so no
backend systematically benefits from a warm cache or a cool CPU. The exact
order is in `report.json`.

**Statistics.** Raw samples are always published. The summary reports n, min,
median, p95, max, mean, sample standard deviation and coefficient of
variation, with the exact definition of each repeated in every summary
object — median is the lower of the two middle samples (so it is a value
that was really observed) and p95 is nearest-rank.

**Cache policy is stated, not implied.** Without `--drop-caches` (which
needs root) reads follow the writes that filled them, so read and restore
numbers are warm-cache numbers. The report says so in those words rather
than leaving a reader to assume otherwise.

**Bounded and owned.** Every child process has a deadline and runs in a
process group of its own; when the deadline fires, or when the child exits
leaving descendants behind, the whole group is terminated. Scratch data
lives only inside a directory the harness created and marked as its own,
`--out` must not already exist, and `clean` will only delete a directory
carrying that marker.

---

## Fairness: what is not comparable

The report prints these next to every number; they are the reason there is no
single ranking.

* **restic encrypts everything it stores** (AES-256-CTR with Poly1305-AES) and
  compresses with zstd. That is real work the Amber cores and Git do not do,
  and it is the main reason restic's CPU column is not comparable with theirs.
* **Neither Amber core encrypts anything.** Both compress records with zstd
  and hash the uncompressed bytes, so compression never affects addressing.
* **The two Amber stores are not comparable by total size.** The reference
  database is redb in Rust and Pebble in Go, and redb preallocates several
  megabytes, which dominates a small profile. The `store_packstore_*` and
  `store_refs_*` counters separate the pack bytes — the number the storage
  comparison is about — from the database file.
* **Neither Amber CLI has a recursive listing**, so the listing operation is
  one `amber-store ls` process per directory. The invocation count is
  recorded and the number is not comparable with a single-process listing
  without allowing for those process startups.
* **Git stores no mtimes, no permission bits beyond `x`, and no empty
  directories**, so trees restored from Git are compared without them, and
  the report lists exactly which attributes were not checked.
* **A Nix store is not a filesystem-tree store.** It enforces a reference
  graph, immutability, signatures and a build model that neither Amber core
  implements. The Nix group is a *storage-layer* comparison; operations that
  depend on the rest (closure queries, binary-cache publication, NAR/
  reference registration) are recorded as unsupported with the reason.
  **Nothing here supports a claim that a tree store can replace a Nix
  store.**
* **Transport differs by backend, and every row says how.** restic and Nix
  speak S3 natively. Git has no native protocol for pushing to arbitrary S3
  object storage, so its publication is the documented bundle route, and the
  upload is the harness's. Neither Amber core ships any network client at
  all, so the harness mirrors the store's on-disk pack segments with `mc` —
  labelled in every row as a harness transport, comparable with Git's dumb
  publication and not with a native S3 client.
* **Neither Amber core can fetch a single reference**, so delivering one
  published tree means downloading the whole store. That is what the
  delivered-set counters record, rather than being hidden.
* **Segment size is set explicitly, smaller than production.** Both cores
  default to 2 GiB segments. A segment is the unit collection reaps, so with
  2 GiB segments a profile that stores less than that could never reclaim
  anything and the GC measurement would be vacuous. Each profile sets a
  smaller size, identically for both cores, and the report states it.
* **Chunking is set explicitly and identically** for both cores on every
  invocation (32 KiB / 512 KiB / 1 MiB, item-bits 7) rather than left to
  defaults. restic's chunker parameters are not exposed on its command line
  and are not equalised; the report says so.

---

## Commands and options

```
amber-cas-bench run --out DIR [options]
amber-cas-bench clean --out DIR      # remove a run's scratch data, keep its reports
amber-cas-bench publish --report DIR # export a finished run into results/
amber-cas-bench profiles             # print the sizes each profile uses
```

| option | meaning |
|---|---|
| `--profile smoke\|standard\|large` | sizes; default `smoke` |
| `--out DIR` | report directory; must not already exist |
| `--seed N` | seed for the corpus and every fixture |
| `--order-seed N` | seed for the randomised backend order (default: derived from `--seed`; always recorded) |
| `--repeats N` | repetitions of every (scenario, backend) pair |
| `--jobs N`, `-j N` | worker threads offered to backends that take a count |
| `--groups tree,git,nix,backup,blob` | scenario groups to run |
| `--scenarios NAME[,NAME]` | scenarios, by full name or suffix |
| `--backends NAME[,NAME]` | backends; a name that appears in no selected scenario is an error, not a silent omission |
| `--timeout-secs N` | per-command deadline |
| `--drop-caches` | drop the page cache first (needs root; the report records whether it worked) |
| `--keep-scratch` | keep the scratch data instead of deleting it |
| `--blob-latency-ms N` | simulated one-way delay injected per HTTP message per direction |
| `--blob-bandwidth-bytes-per-s N` | simulated bandwidth cap per direction |
| `--remote-s3-endpoint URL` + `--remote-s3-bucket B` + `--remote-s3-prefix P` | use a remote S3 endpoint instead of the local service |
| `--amber-rust-bin`, `--amber-go-bin` | the core executables to measure; `run.sh` builds them from the pinned revisions and passes them |
| `--amber-rust-rev SHA`, `--amber-go-rev SHA` | source revision to record for a core built from pinned source |
| `--amber-rust-repo DIR`, `--amber-go-repo DIR` | the local checkout a core was built from; its revision *and* dirty state are recorded instead of a pin |
| `--harness-repo DIR` | checkout of this repository, whose revision and dirty state are recorded as the harness's own (default `.`) |
| `--nix-closure-root PATH` | measure an existing closure instead of the built fixture; give it twice for a second generation (see below) |

`run.sh` passes everything it does not recognise straight through, so any of
these work there too.

### Which sources get measured

The default is both pins, and no checkout of either core has to exist:

| | source | what the report records |
|---|---|---|
| default | `amber-rust-src` and `amber-go-src` in `flake.lock` | the pinned revision, `source_dirty: false` |
| `AMBER_RUST_REPO=/path/to/core-rs ./run.sh` | that checkout, built with `cargo build --release --locked` | its `HEAD` **and** whether the tree was dirty |
| `AMBER_GO_REPO=/path/to/core ./run.sh` | that checkout, built with `go build` | its `HEAD` **and** whether the tree was dirty |

The two overrides are independent: one core can come from a checkout while
the other stays on its pin. Either way the report carries the SHA-256 of the
executable that actually ran, so a number can always be traced back to the
bytes that produced it.

To move a pin, edit `flake.nix` and run `nix flake lock --update-input
amber-rust-src` (or `amber-go-src`). Both cores are also buildable on their
own: `nix build .#amber-rust`, `nix build .#amber-go`.

### Measuring a closure you already have

The Nix group's default input is the deterministic fixture this flake builds,
and that is what the smoke profile and CI use — a benchmark whose input
depends on the machine it runs on is not reproducible. An existing closure
can be measured instead, explicitly:

```
# one generation
./run.sh --groups nix,blob --scenarios nix/closure,blob/nix-closure \
  --nix-closure-root /nix/store/<hash>-<name>

# two generations, so the incremental and retention operations have meaning
./run.sh --groups nix,blob --scenarios nix/closure,blob/nix-closure \
  --nix-closure-root /nix/store/<hash>-my-app-v1 \
  --nix-closure-root /nix/store/<hash>-my-app-v2
```

Supplied paths are **read only**. The harness queries their closure, NAR
hashes and reference lists from the store they are already in (with
substitution off, so a path that is not really there is an error rather than
something fetched from a cache), recomputes every one of those NAR hashes
from the bytes on disk with `nix hash path` before the run starts, and then
copies them into stores of its own. It never writes to a supplied path.

With **two** roots the first is generation 1 and the second is generation 2,
and every operation runs: initial import, repeat import, incremental import,
the object-storage publication and pull pair, and the retention step.

With **one** root there is no second generation, and the operations that need
a change are recorded as `unsupported` with that reason — `import_incremental`
and `delete_path` in `nix/closure`, and `push_incremental`,
`pull_incremental` and `retention_cleanup` in `blob/nix-closure`. Everything
else — import, repeat import, listing, closure query, read-back,
materialisation, binary-cache export and substitution, collection, the
initial publication, the fresh pull, the NAR-hash verification and the
missing-object probe — runs normally. The alternative would be to edit
somebody's store paths to manufacture churn, or to publish the same closure
twice and call the second one incremental; neither is a measurement of
anything, so neither is done.

More than two roots is an error: the workload has exactly two generations.

---

## Profiles and scaling

`amber-cas-bench profiles` prints the exact sizes. Roughly:

| profile | corpus/generation | commits | Nix closure | repeats | segment | free space | time |
|---|---|---|---|---|---|---|---|
| `smoke` | ~33 MiB × 3 | 15 | ~6 MiB | 1 | 8 MiB | ~4 GiB | minutes |
| `standard` | ~2.1 GiB × 3 | 51 | ~180 MiB | 3 | 64 MiB | ~120 GiB | hours |
| `large` | ~17 GiB × 3 | 101 | ~1.4 GiB | 3 | 256 MiB | ~1 TiB | many hours |

The pre-flight check warns (it does not refuse) when the scratch filesystem
has less free space than the profile expects. Scratch for a repetition is
reclaimed as soon as that repetition has been verified, so the peak
requirement is well below the sum of every store.

`smoke` and `standard` restore and compare **every** retained version of the
source history, both in `git/history` and in the `blob/source-history`
publication. `large` samples them — first, last and an even spread between —
and then the checks are named `sampled_retained_versions_match` and
`sampled_fresh_pull_restores_correctly` and report how many versions they did
not look at. A subset never gets to claim it checked everything.

To scale further, add a profile in `src/config.rs`: every size is a literal
there and the whole structure is serialised into the report, so any number
can be traced back to the configuration that produced it. A profile is
required (by a unit test) to keep its segment size small enough for
collection to be able to reclaim something.

---

## Object storage

By default the harness starts its own **Garage** instance (pinned by this
flake), creates a bucket and a key, and points every backend at a **counting
gateway** in front of it. The gateway forwards bytes verbatim while parsing
HTTP/1.1 framing, so it counts requests by method, bodies by direction, and
retries and pre-flight probes as well — they are part of what the transfer
cost. If its parser ever loses the framing it sets `parse_desync`, and the
affected counts are reported as untrustworthy rather than quietly wrong;
byte totals stay exact because they are counted at the socket.

`--blob-latency-ms` and `--blob-bandwidth-bytes-per-s` add a deliberately
coarse simulation, and the exact model is written into the report verbatim so
nobody has to guess what a number meant.

Each repetition publishes under its own key prefix, and the harness's own
listing and cleanup calls bypass the gateway so they are never charged to a
measured operation. Because a store URI is a request rather than a guarantee
— Nix carries a cache's key prefix in the path after the bucket, and only
warns about parameters it does not recognise — the harness lists the bucket
before and after the first native publication and **fails the run if any key
appeared outside the namespace it asked for**.

### A remote endpoint

```
./run.sh --groups blob \
  --remote-s3-endpoint https://s3.example.com \
  --remote-s3-bucket my-bucket \
  --remote-s3-prefix "amber-bench/$(date -u +%Y%m%dT%H%M%SZ)-$RANDOM"
```

Credentials come from `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` and are
passed to child processes in the environment only — never in a command line,
and never written to any output file. A remote run **must** name a unique
prefix: the harness refuses to start without one, never lists, overwrites or
deletes an object outside it, and judges the namespace check only against
objects that appeared during the run, so a bucket with unrelated contents is
safe. An endpoint given without a scheme is treated as HTTPS. A remote
endpoint is reached over TLS, which the gateway cannot look inside, so
request and byte counters are **absent** for such a run rather than
estimated.

---

## Correctness checks

Every repetition verifies what it produced, and a failed check invalidates
the comparison rather than being reported beside it.

| check | what it establishes |
|---|---|
| `restore_matches_manifest`, `post_gc_restore_matches_manifest` | restored bytes, permissions, mtimes and symlink targets equal an independently observed manifest |
| `integrity_check` | the backend's own consistency check over the whole store |
| `retention_took_effect`, `forgotten_snapshots_are_absent`, `dropped_references_are_absent` | what retention kept is listed and restorable, and what it dropped is absent **from the backend's own successful listing** — never inferred from a command that failed |
| `all_retained_versions_match` / `sampled_retained_versions_match` | every (or a named sample of) retained source version checks back out byte for byte |
| `expired_narinfos_are_absent` | the expired keys are gone from a **successful** object listing, *and* a client with its own empty narinfo cache cannot obtain the superseded paths |
| `clone_matches_source`, `incremental_fetch_matches_source` | the *destination* repository passes `git fsck`, holds every branch and tag of the source at the same commit id, and materialises the expected tree |
| `closure_matches_source`, `closure_is_complete` | the destination holds exactly the expected store paths, and every reference of a retained path is retained |
| `nar_hashes_and_references_match`, `nar_hashes_match_recomputed` | NAR hashes and reference lists equal the source store's — registered where the backend registers them, and recomputed from the restored bytes with `nix hash path` where it does not |
| `published_keys_stay_inside_the_namespace` | a native client's writes really landed only where the harness asked |
| `fresh_pull_restores_correctly`, `incremental_pull_restores_correctly`, `retained_content_valid_after_cleanup` | every delivered reference restores from the destination and matches its source (or, under a sampling profile, `sampled_…` with the unverified count) |
| `missing_object_is_detected` | after one published object is deleted, the backend fails rather than returning a short or empty result |
| `amber_cores_agree_on_root_keys` | the two cores produce identical root keys for identical trees — the byte-compatibility claim, checked rather than assumed |

---

## Recording results

A `--out` directory is a working artefact: large, sitting next to scratch
data, full of paths that belong to the machine that produced it. What gets
committed is a portable export of it:

```
./run.sh --profile standard --out /var/tmp/bench/standard-1
nix develop --command cargo run --release -- publish \
  --report /var/tmp/bench/standard-1 \
  --results results \
  --label "standard profile, three repetitions, idle build host"
```

That writes `results/<YYYYMMDDTHHMMSSZ>-<profile>/` containing

* `README.md` — the run's page: what produced the numbers, which
  executables were measured with their revisions and hashes, and the charts;
* `plots/*.svg` — one static chart per scenario and metric;
* `REPORT.md`, `report.json` and the four CSVs, exactly as the run wrote
  them apart from the path redaction below;
* `run.json` — the index entry, which is the only thing
  `results/README.md` is rebuilt from. The index is *derived*, never
  appended to, so deleting a run directory and running `publish --reindex`
  is all it takes to remove it.

Five things it will not do:

* **Publish an invalid run as a result.** A run whose own verdict is invalid
  is refused. `--allow-invalid` records it as a diagnostic instead: marked
  invalid on every page, with no chart drawn for it at all.
* **Overwrite a recorded run.** An existing `results/<id>` is an error.
* **Recompute a measurement.** The JSON and CSV are the ones the run wrote.
* **Put one scenario's bars next to another's.** A chart covers one scenario
  and one metric; an operation a backend cannot perform is written out in
  words rather than drawn as a bar of length zero; and every chart states
  its unit, the sample count behind each bar, the run's cache policy and
  each backend's storage semantics.
* **Commit the directory it ran in.** The run's output, scratch, repository
  and home directories are replaced by `<run-output-dir>`, `<scratch>`,
  `<repo>` and `<home>` everywhere they appear, in every artefact, and the
  page says which placeholders it used. `--redact PATH` adds more. Only
  paths are rewritten; every value, counter, revision and executable hash is
  left exactly as measured.

### Exploring a recorded run

`results/explore.ipynb` loads any recorded run and compares its scenarios —
elapsed time, store size, transferred bytes and request counts, the
operations with no measurement, and the correctness checks. Its dependencies
are pinned by this flake:

```
nix develop --command jupyter lab results/explore.ipynb
# or, headless, against one recorded run:
AMBER_BENCH_RUN=20260912T183000Z-standard \
  nix develop --command jupyter nbconvert --to notebook --execute \
    --output /tmp/executed.ipynb results/explore.ipynb
```

It refuses to compare a run whose own verdict is invalid, prints tables as
text and plots as images rather than widgets, and is executed against every
committed run in CI. Nothing in `results/` needs it: the Markdown and the
SVG are the readable form.

---

## Extending the suite

**A new backend.** Add an adapter under `src/adapters/` that builds `Run`
command lines (never a shell), and a `Semantics` block in
`adapters::semantics` stating its compression, encryption, durability and
concurrency — an unknown backend reports `unrecorded` rather than inventing
guarantees, and a unit test enforces that every shipped backend fills them
in. Then add the name to the relevant scenario's `BACKENDS`, teach that
scenario's `build()` how to construct it, and map it to its required tool in
`run::required_tool`. If it cannot do one of the scenario's operations,
return `Op::unsupported(name, reason)`: that is a first-class outcome, not a
gap to be papered over.

**A new scenario.** Add a module under `src/scenarios/` exposing `GROUP`,
`SCENARIO`, `BACKENDS` and `run(ctx, backend, rep, order_index)`, and list it
in `run::catalogue`. Use `measured`/`measured_many` for what is being
compared and `aside` for anything else, so the phase separation holds.
Verify the result against a manifest: a scenario that measures without
checking is not finished.

**A new metric.** `Op` carries typed fields for timing, CPU, peak RSS,
logical bytes, store occupancy and reclaim, plus arbitrary integer
`counters`. Prefer a counter to overloading an existing field, and add it to
`report::csv` so it reaches `counters.csv`.

Run `cargo test` in this directory for the unit tests (dataset determinism,
adapters, metrics, reports, cleanup, error handling and verification
failures), and `cargo clippy --all-targets -- -D warnings`.

---

## Continuous integration

Three jobs in `.github/workflows/ci.yml`:

* **`unit`** — `cargo fmt --check`, `cargo clippy --all-targets --locked -D
  warnings` and `cargo test --locked`.
* **`smoke`** — `./run.sh --profile smoke`: both cores built from the pinned
  revisions, the **real** smoke suite against every backend, the local
  Garage instance, real HTTP transfers. It then re-reads `report.json` and
  fails unless `validity.valid` is true, there are no errors, no skipped
  backends, no failed check, no failed cross-backend check, no excluded
  invalid samples, **every** backend the suite knows produced at least one
  measured operation — so a backend quietly dropping out of the catalogue
  fails the build — and both cores recorded a 40-character source revision.
  It then publishes the report the way a recorded result is published and
  checks that nothing naming the runner's workspace reached the output. The
  report is kept as a build artefact.
* **`notebook`** — executes `results/explore.ipynb` headlessly against every
  run committed under `results/`.

`smoke` is a correctness gate, not a performance gate: timings from a shared
CI runner are not comparable between runs, and none of them are recorded as
results.

---

## The older micro-benchmarks

`examples/amber-bench.rs` in the Rust core and `cmd/amber-bench` in the Go
core are unchanged and still useful: they are single-implementation
ingest → delete → gc micro-benchmarks with no external dependencies, handy
for a quick check of one core. They are not comparisons, they generate their
own data, and they do not verify what they stored. This suite exists because
answering "should this replace Git, a Nix store or restic?" needs the same
bytes, the same operations, an independent check of every result, and an
explicit account of what each system actually guarantees.
