# Benchmark run `20260912T194132Z-standard`

Standard profile on bld1: 3 repetitions of every (scenario, backend) pair, seed 20260912, warm read cache, local Garage object store. The reference measurement recorded here.

This run declared itself **valid**: no operation failed, no correctness check failed, no cross-backend check failed and no requested backend went unrun. Only repetitions that were healthy throughout contribute to the statistics below.

## What produced these numbers

| | |
|---|---|
| started (UTC) | 2026-09-12T19:41:32Z |
| wall clock | 30.9 min |
| profile | `standard` — ~2.1 GiB per corpus generation, 3 generations, 51 commits, the 180 MiB Nix fixture closure, 3 repetitions by default. Hours. |
| repetitions | 3 per (scenario, backend) |
| seeds | corpus and fixtures `20260912`, randomised backend order `6624854655743516625` |
| worker threads offered | 32 |
| Amber segment size | 64.00 MiB (both cores, explicitly) |
| harness | `amber-cas-bench` 0.1.0, commit `3656de6abe68de2e52a52bde2fb533afefa6730d` |
| host | linux x86_64, AMD Ryzen 9 7950X3D 16-Core Processor (32 logical cores), 125.32 GiB RAM |
| kernel | `6.18.45 #1-NixOS SMP PREEMPT_DYNAMIC Wed Aug 19 16:18:21 UTC 2026` |
| load average at start | 0.81, 1.72, 2.13 (1/5/15 min) |
| cache policy | The page cache was NOT dropped: dropping it needs root, which this harness does not ask for. Every store is freshly created per repetition, and reads follow the writes that filled them, so read and restore numbers are WARM-CACHE numbers and must not be described as cold. Write numbers are unaffected. Enable --drop-caches on a host where the harness can write /proc/sys/vm/drop_caches to change this. |
| object store | local garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] started by the harness — none: the loopback path to the local object store was measured as it is |
| transfer measurement | every backend's S3 traffic is forwarded verbatim through a counting gateway at http://127.0.0.1:44793; requests are counted by method and bodies by direction, including retries. The harness's own listing and cleanup calls bypass it. |
| command | `<repo>/target/release/amber-cas-bench run --harness-repo <repo> --flake-dir <repo> --amber-rust-bin /nix/store/5cqjvnw3zg461xww08q9gj3lwd8v2iz1-amber-store-rust-0.2.0-141df2b0a9a8/bin/amber-store --amber-rust-rev 141df2b0a9a8ad1726769f63811db3a166be188a --amber-go-bin /nix/store/y73dri4kbmb0jx08kdi93cyyxphjk2d6-amber-store-go-0.0.7-4ed4660657b1/bin/amber-store --amber-go-rev 4ed4660657b12421a534ab0b08cfd717ae3d2291 --profile standard --out <run-output-dir>` |

### Executables measured

| tool | version | source revision | SHA-256 of the executable |
|---|---|---|---|
| `amber-go` | not reported by this executable | `4ed4660657b12421a534ab0b08cfd717ae3d2291` | `18f944b7c8a8ead8af406c2ce533135b3677e39157b97031e6974a67d4d0af7b` |
| `amber-rust` | not reported by this executable | `141df2b0a9a8ad1726769f63811db3a166be188a` | `0d86e768712cce7fabc4354f32c32ac1ed4952b21cc7e9131913d97f7ac8a983` |
| `garage` | garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] | — | `2e6718fa730aab04f7021825f50ba30785bc6322cd9d5881025febe59ea81e07` |
| `git` | git version 2.54.0 | — | `27232b9fd6b101c571dde560552ca3ade4102cc68b1e450c6c93279096026a38` |
| `mc` | mc version RELEASE.2025-08-13T08-35-41Z (commit-id=RELEASE.2025-08-13T08-35-41Z) | — | `6476e69f864a45f0ed63f495ef67ce103f0680b6ebb646773b33bfdd0d89745a` |
| `nix` | nix (Nix) 2.34.8 | — | `d9a869cc38d8697b420c1b7180b954412196cb0377b23cfd57a421ea77d57f1c` |
| `restic` | restic 0.18.1 compiled with go1.26.7 on linux/amd64 | — | `b4d2faf4675fd55071032a1097ec33d9d6cc5638256df3dafbd3bb149fd213d5` |

## Charts

One scenario per chart, because that is the only level at which two bars describe the same work. An operation a backend cannot perform is written out as such: it is never drawn as a bar of length zero. Each chart states its unit, the sample count behind every bar, this run's cache policy and the storage semantics of each backend in it.

### `backup/retention`

![plots/backup-retention-elapsed.svg](plots/backup-retention-elapsed.svg)

![plots/backup-retention-store-bytes.svg](plots/backup-retention-store-bytes.svg)

### `blob/backup-corpus`

![plots/blob-backup-corpus-elapsed.svg](plots/blob-backup-corpus-elapsed.svg)

![plots/blob-backup-corpus-upload-bytes.svg](plots/blob-backup-corpus-upload-bytes.svg)

![plots/blob-backup-corpus-download-bytes.svg](plots/blob-backup-corpus-download-bytes.svg)

![plots/blob-backup-corpus-requests.svg](plots/blob-backup-corpus-requests.svg)

### `blob/nix-closure`

![plots/blob-nix-closure-elapsed.svg](plots/blob-nix-closure-elapsed.svg)

![plots/blob-nix-closure-upload-bytes.svg](plots/blob-nix-closure-upload-bytes.svg)

![plots/blob-nix-closure-download-bytes.svg](plots/blob-nix-closure-download-bytes.svg)

![plots/blob-nix-closure-requests.svg](plots/blob-nix-closure-requests.svg)

### `blob/source-history`

![plots/blob-source-history-elapsed.svg](plots/blob-source-history-elapsed.svg)

![plots/blob-source-history-upload-bytes.svg](plots/blob-source-history-upload-bytes.svg)

![plots/blob-source-history-download-bytes.svg](plots/blob-source-history-download-bytes.svg)

![plots/blob-source-history-requests.svg](plots/blob-source-history-requests.svg)

### `git/history`

![plots/git-history-elapsed.svg](plots/git-history-elapsed.svg)

![plots/git-history-store-bytes.svg](plots/git-history-store-bytes.svg)

### `nix/closure`

![plots/nix-closure-elapsed.svg](plots/nix-closure-elapsed.svg)

![plots/nix-closure-store-bytes.svg](plots/nix-closure-store-bytes.svg)

### `tree/lifecycle`

![plots/tree-lifecycle-elapsed.svg](plots/tree-lifecycle-elapsed.svg)

![plots/tree-lifecycle-store-bytes.svg](plots/tree-lifecycle-store-bytes.svg)

## The full result

| file | what it is |
|---|---|
| [`REPORT.md`](REPORT.md) | the readable comparison, scenario by scenario, with every correctness check and every caveat |
| [`report.json`](report.json) | everything, including every raw sample, every executed command and every recorded failure |
| [`samples.csv`](samples.csv) | one row per operation per repetition, with `rep_valid` |
| [`summary.csv`](summary.csv) | the aggregated statistics, with `run_valid` on every row |
| [`counters.csv`](counters.csv) | every integer counter: S3 requests by method, bytes by direction, store component sizes, delivered references |
| [`verifications.csv`](verifications.csv) | one row per correctness check |
| [`run.json`](run.json) | this run's index entry, which is what the results index is built from |

Statistics: median = lower of the two middle samples for even n; p95 = nearest-rank (sorted ascending, index ceil(0.95*n)-1); stddev = sample standard deviation with n-1 denominator (null for n < 2); cv = stddev/mean

The directories this run used are not part of the result, so publishing replaced them everywhere they appeared — in the command line, the tool paths and the charts — with `<repo>`, `<home>`, `<scratch>`, `<run-output-dir>`. Nothing else was rewritten: every measured value, counter, revision and executable hash is the one the run recorded.

[Back to the results index](../README.md)
