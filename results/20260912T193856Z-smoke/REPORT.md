# Amber-Store CAS comparison benchmarks

Run `smoke-1789241936307339195`, profile **smoke**, seed `20260913`, 2 repetition(s), 32 worker(s).

~35 MiB per corpus generation, 3 generations, 14 commits, the 5.7 MiB Nix fixture closure, 1 repetition by default. Minutes.

## How to read this

* Every row is a **median over repetitions** of one operation by one backend. The raw samples are in `samples.csv`; the dispersion is in `summary.csv`.
* An empty cell means **not measured**. An operation a backend cannot perform is listed under [Unsupported operations](#unsupported-operations) with the reason, and is never shown as a zero.
* Comparisons are **within a scenario only**. Backends in different scenarios publish different data and make different guarantees; see [What each backend guarantees](#what-each-backend-guarantees) before comparing anything across sections.
* Cache policy: The page cache was NOT dropped: dropping it needs root, which this harness does not ask for. Every store is freshly created per repetition, and reads follow the writes that filled them, so read and restore numbers are WARM-CACHE numbers and must not be described as cold. Write numbers are unaffected. Enable --drop-caches on a host where the harness can write /proc/sys/vm/drop_caches to change this.
* Statistics: median = lower of the two middle samples for even n; p95 = nearest-rank (sorted ascending, index ceil(0.95*n)-1); stddev = sample standard deviation with n-1 denominator (null for n < 2); cv = stddev/mean
* Setup (generating the corpus, materialising a working tree) and verification (re-hashing a restored tree) are timed separately and are never part of a measured number.
* Only repetitions in which **everything** succeeded contribute to a median: a fast operation in a repetition whose restored bytes did not match its manifest is not a measurement of anything. Such samples are counted as `excluded_invalid_samples` in `summary.csv` and kept in full in `samples.csv`.

## Scenario `blob/nix-closure`

### Elapsed time (median, lower is better)

| operation | amber-go-s3 | nix-binary-cache | amber-rust-s3 |
|---|---|---|---|
| `push_initial` | 52.7 ms | 363.5 ms | 51.5 ms |
| `push_noop` | 18.0 ms | 19.8 ms | 17.9 ms |
| `pull_fresh` | 24.9 ms | 100.3 ms | 23.1 ms |
| `push_incremental` | 57.8 ms | 131.2 ms | 54.2 ms |
| `pull_incremental` | 28.7 ms | 50.8 ms | 22.7 ms |
| `retention_cleanup` | 58.6 ms | 37.3 ms | 53.8 ms |

### Throughput over the logical bytes processed (median)

| operation | amber-go-s3 | nix-binary-cache | amber-rust-s3 |
|---|---|---|---|
| `push_initial` | 57.83 MiB/s | 9.02 MiB/s | 129.69 MiB/s |
| `push_noop` | 336.55 KiB/s | 0 B/s | 134.53 KiB/s |
| `pull_fresh` | 130.44 MiB/s | 27.27 MiB/s | 279.67 MiB/s |
| `push_incremental` | 65.33 MiB/s | 3.89 MiB/s | 132.31 MiB/s |
| `pull_incremental` | 56.44 MiB/s | 5.54 MiB/s | 113.06 MiB/s |
| `retention_cleanup` | 64.66 MiB/s | 208.56 KiB/s | 132.37 MiB/s |

### CPU time, user + system (median)

| operation | amber-go-s3 | nix-binary-cache | amber-rust-s3 |
|---|---|---|---|
| `push_initial` | 44.4 ms | 610.0 ms | 35.3 ms |
| `push_noop` | 24.6 ms | 17.5 ms | 24.3 ms |
| `pull_fresh` | 41.6 ms | 69.6 ms | 28.6 ms |
| `push_incremental` | 53.1 ms | 157.5 ms | 36.1 ms |
| `pull_incremental` | 46.1 ms | 51.9 ms | 30.8 ms |
| `retention_cleanup` | 50.7 ms | 48.6 ms | 36.7 ms |

### Peak resident set size (median)

| operation | amber-go-s3 | nix-binary-cache | amber-rust-s3 |
|---|---|---|---|
| `push_initial` | 38.58 MiB | 131.75 MiB | 38.58 MiB |
| `push_noop` | 38.58 MiB | 38.58 MiB | 38.58 MiB |
| `pull_fresh` | 38.58 MiB | 50.30 MiB | 38.58 MiB |
| `push_incremental` | 38.58 MiB | 77.91 MiB | 38.58 MiB |
| `pull_incremental` | 38.58 MiB | 44.43 MiB | 38.58 MiB |
| `retention_cleanup` | 38.58 MiB | 38.58 MiB | 38.58 MiB |

### Dispersion of elapsed time

| backend | operation | n | median | p95 | min | max | stddev | cv |
|---|---|---|---|---|---|---|---|---|
| amber-go-s3 | `push_initial` | 2 | 52.7 ms | 56.9 ms | 52.7 ms | 56.9 ms | 3.0 ms | 5.5% |
| amber-go-s3 | `push_noop` | 2 | 18.0 ms | 18.8 ms | 18.0 ms | 18.8 ms | 570.8 µs | 3.1% |
| amber-go-s3 | `pull_fresh` | 2 | 24.9 ms | 25.2 ms | 24.9 ms | 25.2 ms | 244.8 µs | 1.0% |
| amber-go-s3 | `push_incremental` | 2 | 57.8 ms | 58.4 ms | 57.8 ms | 58.4 ms | 453.3 µs | 0.8% |
| amber-go-s3 | `pull_incremental` | 2 | 28.7 ms | 67.3 ms | 28.7 ms | 67.3 ms | 27.3 ms | 56.8% |
| amber-go-s3 | `retention_cleanup` | 2 | 58.6 ms | 59.0 ms | 58.6 ms | 59.0 ms | 248.3 µs | 0.4% |
| nix-binary-cache | `push_initial` | 2 | 363.5 ms | 364.8 ms | 363.5 ms | 364.8 ms | 950.4 µs | 0.3% |
| nix-binary-cache | `push_noop` | 2 | 19.8 ms | 21.3 ms | 19.8 ms | 21.3 ms | 1.0 ms | 5.1% |
| nix-binary-cache | `pull_fresh` | 2 | 100.3 ms | 120.3 ms | 100.3 ms | 120.3 ms | 14.1 ms | 12.8% |
| nix-binary-cache | `push_incremental` | 2 | 131.2 ms | 132.2 ms | 131.2 ms | 132.2 ms | 733.0 µs | 0.6% |
| nix-binary-cache | `pull_incremental` | 2 | 50.8 ms | 91.3 ms | 50.8 ms | 91.3 ms | 28.7 ms | 40.3% |
| nix-binary-cache | `retention_cleanup` | 2 | 37.3 ms | 37.4 ms | 37.3 ms | 37.4 ms | 50.1 µs | 0.1% |
| amber-rust-s3 | `push_initial` | 2 | 51.5 ms | 52.3 ms | 51.5 ms | 52.3 ms | 593.0 µs | 1.1% |
| amber-rust-s3 | `push_noop` | 2 | 17.9 ms | 19.2 ms | 17.9 ms | 19.2 ms | 919.0 µs | 4.9% |
| amber-rust-s3 | `pull_fresh` | 2 | 23.1 ms | 24.3 ms | 23.1 ms | 24.3 ms | 819.3 µs | 3.5% |
| amber-rust-s3 | `push_incremental` | 2 | 54.2 ms | 55.1 ms | 54.2 ms | 55.1 ms | 669.9 µs | 1.2% |
| amber-rust-s3 | `pull_incremental` | 2 | 22.7 ms | 64.4 ms | 22.7 ms | 64.4 ms | 29.5 ms | 67.8% |
| amber-rust-s3 | `retention_cleanup` | 2 | 53.8 ms | 55.1 ms | 53.8 ms | 55.1 ms | 940.2 µs | 1.7% |

### What these numbers mean, operation by operation

Read these before comparing two cells above: they state what each measured operation actually delivered.

| backend | operation | note |
|---|---|---|
| amber-go-s3 | `pull_fresh` | delivered to the destination: the whole published store, which holds all 8 reference(s) published so far; these cores have no selective fetch |
| amber-go-s3 | `pull_fresh` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `pull_incremental` | delivered to the destination: the store's new pack segments and reference database, bringing the client up to the 9 reference(s) the incremental publication added |
| amber-go-s3 | `pull_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `push_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `push_initial` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `push_noop` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `retention_cleanup` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `retention_cleanup` | retained by this retention step: the 9 reference(s) of the incremental publication; the 2 of the initial one were dropped and collected. Retention is the one operation in this scenario where the backends are not asked for the same thing — consolidating Git bundles still publishes the whole history, while dropping an Amber reference, forgetting a restic snapshot or expiring a narinfo removes a version — so these timings are not a like-for-like ranking. What each one kept is stated here, and every member of it is verified afterwards. |
| amber-rust-s3 | `pull_fresh` | delivered to the destination: the whole published store, which holds all 8 reference(s) published so far; these cores have no selective fetch |
| amber-rust-s3 | `pull_fresh` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `pull_incremental` | delivered to the destination: the store's new pack segments and reference database, bringing the client up to the 9 reference(s) the incremental publication added |
| amber-rust-s3 | `pull_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `push_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `push_initial` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `push_noop` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `retention_cleanup` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `retention_cleanup` | retained by this retention step: the 9 reference(s) of the incremental publication; the 2 of the initial one were dropped and collected. Retention is the one operation in this scenario where the backends are not asked for the same thing — consolidating Git bundles still publishes the whole history, while dropping an Amber reference, forgetting a restic snapshot or expiring a narinfo removes a version — so these timings are not a like-for-like ranking. What each one kept is stated here, and every member of it is verified afterwards. |
| nix-binary-cache | `pull_fresh` | delivered to the destination: generation 1's whole closure: 8 store paths |
| nix-binary-cache | `pull_fresh` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| nix-binary-cache | `pull_incremental` | delivered to the destination: generation 2's whole closure: 9 store paths, 6 of which the store already held |
| nix-binary-cache | `pull_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| nix-binary-cache | `push_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| nix-binary-cache | `push_initial` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| nix-binary-cache | `push_noop` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| nix-binary-cache | `retention_cleanup` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| nix-binary-cache | `retention_cleanup` | retained by this retention step: generation 2's whole closure: 9 store paths. 2 narinfo object(s) of paths only generation 1 needed were expired. Retention is the one operation in this scenario where the backends are not asked for the same thing — consolidating Git bundles still publishes the whole history, while dropping an Amber reference, forgetting a restic snapshot or expiring a narinfo removes a version — so these timings are not a like-for-like ranking. What each one kept is stated here, and every member of it is verified afterwards. |

### Recorded counters (median per operation)

| backend | operation | counter | median |
|---|---|---|---|
| amber-go-s3 | `push_initial` | `s3_bytes_down` | 3.76 KiB |
| amber-go-s3 | `push_initial` | `s3_bytes_up` | 3.29 MiB |
| amber-go-s3 | `push_initial` | `s3_connections` | 9 |
| amber-go-s3 | `push_initial` | `s3_request_body_bytes` | 3.28 MiB |
| amber-go-s3 | `push_initial` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_initial` | `s3_requests_PUT` | 15 |
| amber-go-s3 | `push_initial` | `s3_requests_total` | 17 |
| amber-go-s3 | `push_initial` | `s3_response_body_bytes` | 614 B |
| amber-go-s3 | `push_initial` | `s3_responses_2xx` | 16 |
| amber-go-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_initial` | `stored_bytes` | 3.27 MiB |
| amber-go-s3 | `push_initial` | `stored_objects` | 15 |
| amber-go-s3 | `push_noop` | `s3_bytes_down` | 5.16 KiB |
| amber-go-s3 | `push_noop` | `s3_bytes_up` | 1.16 KiB |
| amber-go-s3 | `push_noop` | `s3_connections` | 1 |
| amber-go-s3 | `push_noop` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `push_noop` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_noop` | `s3_requests_total` | 2 |
| amber-go-s3 | `push_noop` | `s3_response_body_bytes` | 4.94 KiB |
| amber-go-s3 | `push_noop` | `s3_responses_2xx` | 1 |
| amber-go-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_noop` | `stored_bytes` | 3.27 MiB |
| amber-go-s3 | `push_noop` | `stored_objects` | 15 |
| amber-go-s3 | `pull_fresh` | `delivered_logical_bytes` | 5.50 MiB |
| amber-go-s3 | `pull_fresh` | `delivered_references` | 8 |
| amber-go-s3 | `pull_fresh` | `s3_bytes_down` | 3.28 MiB |
| amber-go-s3 | `pull_fresh` | `s3_bytes_up` | 12.08 KiB |
| amber-go-s3 | `pull_fresh` | `s3_connections` | 11 |
| amber-go-s3 | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `pull_fresh` | `s3_requests_GET` | 19 |
| amber-go-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| amber-go-s3 | `pull_fresh` | `s3_requests_total` | 20 |
| amber-go-s3 | `pull_fresh` | `s3_response_body_bytes` | 3.28 MiB |
| amber-go-s3 | `pull_fresh` | `s3_responses_2xx` | 18 |
| amber-go-s3 | `pull_fresh` | `s3_responses_4xx` | 2 |
| amber-go-s3 | `pull_fresh` | `stored_bytes` | 3.27 MiB |
| amber-go-s3 | `pull_fresh` | `stored_objects` | 15 |
| amber-go-s3 | `push_incremental` | `s3_bytes_down` | 14.39 KiB |
| amber-go-s3 | `push_incremental` | `s3_bytes_up` | 3.80 MiB |
| amber-go-s3 | `push_incremental` | `s3_connections` | 12 |
| amber-go-s3 | `push_incremental` | `s3_request_body_bytes` | 3.78 MiB |
| amber-go-s3 | `push_incremental` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_incremental` | `s3_requests_POST` | 12 |
| amber-go-s3 | `push_incremental` | `s3_requests_PUT` | 15 |
| amber-go-s3 | `push_incremental` | `s3_requests_total` | 29 |
| amber-go-s3 | `push_incremental` | `s3_response_body_bytes` | 9.96 KiB |
| amber-go-s3 | `push_incremental` | `s3_responses_2xx` | 28 |
| amber-go-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_incremental` | `stored_bytes` | 3.77 MiB |
| amber-go-s3 | `push_incremental` | `stored_objects` | 16 |
| amber-go-s3 | `pull_incremental` | `delivered_logical_bytes` | 5.75 MiB |
| amber-go-s3 | `pull_incremental` | `delivered_references` | 9 |
| amber-go-s3 | `pull_incremental` | `s3_bytes_down` | 3.78 MiB |
| amber-go-s3 | `pull_incremental` | `s3_bytes_up` | 12.05 KiB |
| amber-go-s3 | `pull_incremental` | `s3_connections` | 8 |
| amber-go-s3 | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `pull_incremental` | `s3_requests_GET` | 19 |
| amber-go-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| amber-go-s3 | `pull_incremental` | `s3_requests_total` | 20 |
| amber-go-s3 | `pull_incremental` | `s3_response_body_bytes` | 3.78 MiB |
| amber-go-s3 | `pull_incremental` | `s3_responses_2xx` | 18 |
| amber-go-s3 | `pull_incremental` | `s3_responses_4xx` | 2 |
| amber-go-s3 | `pull_incremental` | `stored_bytes` | 3.77 MiB |
| amber-go-s3 | `pull_incremental` | `stored_objects` | 16 |
| amber-go-s3 | `retention_cleanup` | `retained_logical_bytes` | 5.75 MiB |
| amber-go-s3 | `retention_cleanup` | `retained_references` | 9 |
| amber-go-s3 | `retention_cleanup` | `s3_bytes_down` | 14.56 KiB |
| amber-go-s3 | `retention_cleanup` | `s3_bytes_up` | 3.80 MiB |
| amber-go-s3 | `retention_cleanup` | `s3_connections` | 14 |
| amber-go-s3 | `retention_cleanup` | `s3_request_body_bytes` | 3.78 MiB |
| amber-go-s3 | `retention_cleanup` | `s3_requests_GET` | 2 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_POST` | 14 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_PUT` | 9 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_total` | 25 |
| amber-go-s3 | `retention_cleanup` | `s3_response_body_bytes` | 11.10 KiB |
| amber-go-s3 | `retention_cleanup` | `s3_responses_2xx` | 24 |
| amber-go-s3 | `retention_cleanup` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `retention_cleanup` | `stored_bytes` | 3.78 MiB |
| amber-go-s3 | `retention_cleanup` | `stored_objects` | 10 |
| nix-binary-cache | `push_initial` | `s3_bytes_down` | 7.71 KiB |
| nix-binary-cache | `push_initial` | `s3_bytes_up` | 3.28 MiB |
| nix-binary-cache | `push_initial` | `s3_connections` | 8 |
| nix-binary-cache | `push_initial` | `s3_request_body_bytes` | 3.26 MiB |
| nix-binary-cache | `push_initial` | `s3_requests_GET` | 9 |
| nix-binary-cache | `push_initial` | `s3_requests_HEAD` | 8 |
| nix-binary-cache | `push_initial` | `s3_requests_PUT` | 17 |
| nix-binary-cache | `push_initial` | `s3_requests_total` | 34 |
| nix-binary-cache | `push_initial` | `s3_response_body_bytes` | 2.44 KiB |
| nix-binary-cache | `push_initial` | `s3_responses_1xx` | 1 |
| nix-binary-cache | `push_initial` | `s3_responses_2xx` | 17 |
| nix-binary-cache | `push_initial` | `s3_responses_4xx` | 17 |
| nix-binary-cache | `push_initial` | `stored_bytes` | 3.26 MiB |
| nix-binary-cache | `push_initial` | `stored_objects` | 17 |
| nix-binary-cache | `push_noop` | `s3_bytes_down` | 0 B |
| nix-binary-cache | `push_noop` | `s3_bytes_up` | 0 B |
| nix-binary-cache | `push_noop` | `s3_connections` | 0 |
| nix-binary-cache | `push_noop` | `s3_request_body_bytes` | 0 B |
| nix-binary-cache | `push_noop` | `s3_requests_total` | 0 |
| nix-binary-cache | `push_noop` | `s3_response_body_bytes` | 0 B |
| nix-binary-cache | `push_noop` | `stored_bytes` | 3.26 MiB |
| nix-binary-cache | `push_noop` | `stored_objects` | 17 |
| nix-binary-cache | `pull_fresh` | `delivered_logical_bytes` | 5.50 MiB |
| nix-binary-cache | `pull_fresh` | `delivered_references` | 8 |
| nix-binary-cache | `pull_fresh` | `s3_bytes_down` | 3.27 MiB |
| nix-binary-cache | `pull_fresh` | `s3_bytes_up` | 12.05 KiB |
| nix-binary-cache | `pull_fresh` | `s3_connections` | 5 |
| nix-binary-cache | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| nix-binary-cache | `pull_fresh` | `s3_requests_GET` | 17 |
| nix-binary-cache | `pull_fresh` | `s3_requests_HEAD` | 3 |
| nix-binary-cache | `pull_fresh` | `s3_requests_total` | 20 |
| nix-binary-cache | `pull_fresh` | `s3_response_body_bytes` | 3.26 MiB |
| nix-binary-cache | `pull_fresh` | `s3_responses_2xx` | 20 |
| nix-binary-cache | `pull_fresh` | `stored_bytes` | 3.26 MiB |
| nix-binary-cache | `pull_fresh` | `stored_objects` | 17 |
| nix-binary-cache | `push_incremental` | `s3_bytes_down` | 2.67 KiB |
| nix-binary-cache | `push_incremental` | `s3_bytes_up` | 524.21 KiB |
| nix-binary-cache | `push_incremental` | `s3_connections` | 2 |
| nix-binary-cache | `push_incremental` | `s3_request_body_bytes` | 516.76 KiB |
| nix-binary-cache | `push_incremental` | `s3_requests_GET` | 3 |
| nix-binary-cache | `push_incremental` | `s3_requests_HEAD` | 3 |
| nix-binary-cache | `push_incremental` | `s3_requests_PUT` | 6 |
| nix-binary-cache | `push_incremental` | `s3_requests_total` | 12 |
| nix-binary-cache | `push_incremental` | `s3_response_body_bytes` | 840 B |
| nix-binary-cache | `push_incremental` | `s3_responses_2xx` | 6 |
| nix-binary-cache | `push_incremental` | `s3_responses_4xx` | 6 |
| nix-binary-cache | `push_incremental` | `stored_bytes` | 3.77 MiB |
| nix-binary-cache | `push_incremental` | `stored_objects` | 23 |
| nix-binary-cache | `pull_incremental` | `delivered_logical_bytes` | 5.75 MiB |
| nix-binary-cache | `pull_incremental` | `delivered_references` | 9 |
| nix-binary-cache | `pull_incremental` | `s3_bytes_down` | 515.87 KiB |
| nix-binary-cache | `pull_incremental` | `s3_bytes_up` | 1.85 KiB |
| nix-binary-cache | `pull_incremental` | `s3_connections` | 1 |
| nix-binary-cache | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| nix-binary-cache | `pull_incremental` | `s3_requests_GET` | 3 |
| nix-binary-cache | `pull_incremental` | `s3_requests_total` | 3 |
| nix-binary-cache | `pull_incremental` | `s3_response_body_bytes` | 515.21 KiB |
| nix-binary-cache | `pull_incremental` | `s3_responses_2xx` | 3 |
| nix-binary-cache | `pull_incremental` | `stored_bytes` | 3.77 MiB |
| nix-binary-cache | `pull_incremental` | `stored_objects` | 23 |
| nix-binary-cache | `retention_cleanup` | `retained_logical_bytes` | 5.75 MiB |
| nix-binary-cache | `retention_cleanup` | `retained_references` | 9 |
| nix-binary-cache | `retention_cleanup` | `s3_bytes_down` | 2.70 KiB |
| nix-binary-cache | `retention_cleanup` | `s3_bytes_up` | 5.10 KiB |
| nix-binary-cache | `retention_cleanup` | `s3_connections` | 2 |
| nix-binary-cache | `retention_cleanup` | `s3_request_body_bytes` | 354 B |
| nix-binary-cache | `retention_cleanup` | `s3_requests_GET` | 2 |
| nix-binary-cache | `retention_cleanup` | `s3_requests_HEAD` | 4 |
| nix-binary-cache | `retention_cleanup` | `s3_requests_POST` | 2 |
| nix-binary-cache | `retention_cleanup` | `s3_requests_total` | 8 |
| nix-binary-cache | `retention_cleanup` | `s3_response_body_bytes` | 1.39 KiB |
| nix-binary-cache | `retention_cleanup` | `s3_responses_2xx` | 6 |
| nix-binary-cache | `retention_cleanup` | `s3_responses_4xx` | 2 |
| nix-binary-cache | `retention_cleanup` | `stored_bytes` | 3.77 MiB |
| nix-binary-cache | `retention_cleanup` | `stored_objects` | 21 |
| amber-rust-s3 | `push_initial` | `s3_bytes_down` | 1.21 KiB |
| amber-rust-s3 | `push_initial` | `s3_bytes_up` | 6.79 MiB |
| amber-rust-s3 | `push_initial` | `s3_connections` | 2 |
| amber-rust-s3 | `push_initial` | `s3_request_body_bytes` | 6.78 MiB |
| amber-rust-s3 | `push_initial` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_initial` | `s3_requests_PUT` | 2 |
| amber-rust-s3 | `push_initial` | `s3_requests_total` | 4 |
| amber-rust-s3 | `push_initial` | `s3_response_body_bytes` | 616 B |
| amber-rust-s3 | `push_initial` | `s3_responses_2xx` | 3 |
| amber-rust-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_initial` | `stored_bytes` | 6.78 MiB |
| amber-rust-s3 | `push_initial` | `stored_objects` | 2 |
| amber-rust-s3 | `push_noop` | `s3_bytes_down` | 1.42 KiB |
| amber-rust-s3 | `push_noop` | `s3_bytes_up` | 1.17 KiB |
| amber-rust-s3 | `push_noop` | `s3_connections` | 1 |
| amber-rust-s3 | `push_noop` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `push_noop` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_noop` | `s3_requests_total` | 2 |
| amber-rust-s3 | `push_noop` | `s3_response_body_bytes` | 1.20 KiB |
| amber-rust-s3 | `push_noop` | `s3_responses_2xx` | 1 |
| amber-rust-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_noop` | `stored_bytes` | 6.78 MiB |
| amber-rust-s3 | `push_noop` | `stored_objects` | 2 |
| amber-rust-s3 | `pull_fresh` | `delivered_logical_bytes` | 5.50 MiB |
| amber-rust-s3 | `pull_fresh` | `delivered_references` | 8 |
| amber-rust-s3 | `pull_fresh` | `s3_bytes_down` | 6.78 MiB |
| amber-rust-s3 | `pull_fresh` | `s3_bytes_up` | 4.44 KiB |
| amber-rust-s3 | `pull_fresh` | `s3_connections` | 2 |
| amber-rust-s3 | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `pull_fresh` | `s3_requests_GET` | 6 |
| amber-rust-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| amber-rust-s3 | `pull_fresh` | `s3_requests_total` | 7 |
| amber-rust-s3 | `pull_fresh` | `s3_response_body_bytes` | 6.78 MiB |
| amber-rust-s3 | `pull_fresh` | `s3_responses_2xx` | 5 |
| amber-rust-s3 | `pull_fresh` | `s3_responses_4xx` | 2 |
| amber-rust-s3 | `pull_fresh` | `stored_bytes` | 6.78 MiB |
| amber-rust-s3 | `pull_fresh` | `stored_objects` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_bytes_down` | 1.81 KiB |
| amber-rust-s3 | `push_incremental` | `s3_bytes_up` | 7.29 MiB |
| amber-rust-s3 | `push_incremental` | `s3_connections` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_request_body_bytes` | 7.29 MiB |
| amber-rust-s3 | `push_incremental` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_requests_PUT` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_requests_total` | 4 |
| amber-rust-s3 | `push_incremental` | `s3_response_body_bytes` | 1.20 KiB |
| amber-rust-s3 | `push_incremental` | `s3_responses_2xx` | 3 |
| amber-rust-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_incremental` | `stored_bytes` | 7.28 MiB |
| amber-rust-s3 | `push_incremental` | `stored_objects` | 2 |
| amber-rust-s3 | `pull_incremental` | `delivered_logical_bytes` | 5.75 MiB |
| amber-rust-s3 | `pull_incremental` | `delivered_references` | 9 |
| amber-rust-s3 | `pull_incremental` | `s3_bytes_down` | 7.28 MiB |
| amber-rust-s3 | `pull_incremental` | `s3_bytes_up` | 4.44 KiB |
| amber-rust-s3 | `pull_incremental` | `s3_connections` | 2 |
| amber-rust-s3 | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `pull_incremental` | `s3_requests_GET` | 6 |
| amber-rust-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| amber-rust-s3 | `pull_incremental` | `s3_requests_total` | 7 |
| amber-rust-s3 | `pull_incremental` | `s3_response_body_bytes` | 7.28 MiB |
| amber-rust-s3 | `pull_incremental` | `s3_responses_2xx` | 5 |
| amber-rust-s3 | `pull_incremental` | `s3_responses_4xx` | 2 |
| amber-rust-s3 | `pull_incremental` | `stored_bytes` | 7.28 MiB |
| amber-rust-s3 | `pull_incremental` | `stored_objects` | 2 |
| amber-rust-s3 | `retention_cleanup` | `retained_logical_bytes` | 5.75 MiB |
| amber-rust-s3 | `retention_cleanup` | `retained_references` | 9 |
| amber-rust-s3 | `retention_cleanup` | `s3_bytes_down` | 2.36 KiB |
| amber-rust-s3 | `retention_cleanup` | `s3_bytes_up` | 7.30 MiB |
| amber-rust-s3 | `retention_cleanup` | `s3_connections` | 3 |
| amber-rust-s3 | `retention_cleanup` | `s3_request_body_bytes` | 7.29 MiB |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_POST` | 1 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_PUT` | 2 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_total` | 5 |
| amber-rust-s3 | `retention_cleanup` | `s3_response_body_bytes` | 1.64 KiB |
| amber-rust-s3 | `retention_cleanup` | `s3_responses_2xx` | 4 |
| amber-rust-s3 | `retention_cleanup` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `retention_cleanup` | `stored_bytes` | 7.28 MiB |
| amber-rust-s3 | `retention_cleanup` | `stored_objects` | 2 |

## Scenario `nix/closure`

### Elapsed time (median, lower is better)

| operation | amber-go | nix | amber-rust |
|---|---|---|---|
| `import_initial` | 109.0 ms | 27.1 ms | 55.4 ms |
| `import_repeat` | 70.9 ms | 20.5 ms | 58.3 ms |
| `import_incremental` | 94.1 ms | 22.2 ms | 62.7 ms |
| `catalog_list` | 7.2 ms | 15.2 ms | 5.0 ms |
| `closure_query` | unsupported | 15.6 ms | unsupported |
| `nar_and_reference_registration` | unsupported | — | unsupported |
| `read_closure` | 66.9 ms | 131.3 ms | 47.1 ms |
| `cache_export` | unsupported | 314.5 ms | unsupported |
| `materialize_closure` | 71.3 ms | 22.4 ms | 47.5 ms |
| `cache_substitute` | unsupported | 22.6 ms | unsupported |
| `delete_path` | 6.9 ms | 18.4 ms | 5.9 ms |
| `gc` | 7.9 ms | 19.2 ms | 5.1 ms |

### Throughput over the logical bytes processed (median)

| operation | amber-go | nix | amber-rust |
|---|---|---|---|
| `import_initial` | 50.20 MiB/s | 194.94 MiB/s | 95.96 MiB/s |
| `import_repeat` | 75.39 MiB/s | 261.73 MiB/s | 93.87 MiB/s |
| `import_incremental` | 60.78 MiB/s | 255.27 MiB/s | 85.73 MiB/s |
| `catalog_list` | — | — | — |
| `closure_query` | unsupported | — | unsupported |
| `nar_and_reference_registration` | unsupported | — | unsupported |
| `read_closure` | 84.78 MiB/s | 42.27 MiB/s | 117.73 MiB/s |
| `cache_export` | unsupported | — | unsupported |
| `materialize_closure` | 80.13 MiB/s | 255.82 MiB/s | 102.88 MiB/s |
| `cache_substitute` | unsupported | — | unsupported |
| `delete_path` | — | — | — |
| `gc` | — | — | — |

### CPU time, user + system (median)

| operation | amber-go | nix | amber-rust |
|---|---|---|---|
| `import_initial` | 344.6 ms | 29.5 ms | 112.7 ms |
| `import_repeat` | 113.5 ms | 17.8 ms | 100.0 ms |
| `import_incremental` | 173.8 ms | 20.5 ms | 113.9 ms |
| `catalog_list` | 9.0 ms | 15.9 ms | 4.6 ms |
| `closure_query` | unsupported | 16.3 ms | unsupported |
| `nar_and_reference_registration` | unsupported | — | unsupported |
| `read_closure` | 87.4 ms | 136.1 ms | 43.7 ms |
| `cache_export` | unsupported | 625.2 ms | unsupported |
| `materialize_closure` | 92.8 ms | 29.3 ms | 43.6 ms |
| `cache_substitute` | unsupported | 33.3 ms | unsupported |
| `delete_path` | 8.8 ms | 18.7 ms | 5.5 ms |
| `gc` | 10.2 ms | 19.6 ms | 4.6 ms |

### Peak resident set size (median)

| operation | amber-go | nix | amber-rust |
|---|---|---|---|
| `import_initial` | 70.77 MiB | 38.66 MiB | 38.66 MiB |
| `import_repeat` | 38.58 MiB | 38.66 MiB | 38.66 MiB |
| `import_incremental` | 38.90 MiB | 38.66 MiB | 38.66 MiB |
| `catalog_list` | 38.58 MiB | 38.66 MiB | 38.66 MiB |
| `closure_query` | unsupported | 38.66 MiB | unsupported |
| `nar_and_reference_registration` | unsupported | — | unsupported |
| `read_closure` | 38.58 MiB | 38.66 MiB | 38.66 MiB |
| `cache_export` | unsupported | 142.71 MiB | unsupported |
| `materialize_closure` | 38.58 MiB | 38.74 MiB | 38.66 MiB |
| `cache_substitute` | unsupported | 43.54 MiB | unsupported |
| `delete_path` | 38.58 MiB | 38.72 MiB | 38.66 MiB |
| `gc` | 38.58 MiB | 38.66 MiB | 38.66 MiB |

### Storage allocated after each operation (median)

| operation | amber-go | nix | amber-rust |
|---|---|---|---|
| `import_initial` | 7.72 MiB | 13.79 MiB | 4.83 MiB |
| `import_repeat` | 7.72 MiB | 13.79 MiB | 4.83 MiB |
| `import_incremental` | 8.22 MiB | 16.33 MiB | 5.34 MiB |
| `catalog_list` | — | — | — |
| `closure_query` | unsupported | — | unsupported |
| `nar_and_reference_registration` | unsupported | — | unsupported |
| `read_closure` | — | — | — |
| `cache_export` | unsupported | — | unsupported |
| `materialize_closure` | — | — | — |
| `cache_substitute` | unsupported | — | unsupported |
| `delete_path` | 8.22 MiB | 8.07 MiB | 5.34 MiB |
| `gc` | 3.81 MiB | 6.06 MiB | 5.34 MiB |

### Space reclaimed (median; positive means released)

| operation | amber-go | nix | amber-rust |
|---|---|---|---|
| `import_initial` | — | — | — |
| `import_repeat` | — | — | — |
| `import_incremental` | — | — | — |
| `catalog_list` | — | — | — |
| `closure_query` | unsupported | — | unsupported |
| `nar_and_reference_registration` | unsupported | — | unsupported |
| `read_closure` | — | — | — |
| `cache_export` | unsupported | — | unsupported |
| `materialize_closure` | — | — | — |
| `cache_substitute` | unsupported | — | unsupported |
| `delete_path` | — | — | — |
| `gc` | 4.41 MiB | 2.01 MiB | -4.00 KiB |

### Dispersion of elapsed time

| backend | operation | n | median | p95 | min | max | stddev | cv |
|---|---|---|---|---|---|---|---|---|
| amber-go | `import_initial` | 2 | 109.0 ms | 109.6 ms | 109.0 ms | 109.6 ms | 451.2 µs | 0.4% |
| amber-go | `import_repeat` | 2 | 70.9 ms | 73.0 ms | 70.9 ms | 73.0 ms | 1.4 ms | 2.0% |
| amber-go | `import_incremental` | 2 | 94.1 ms | 94.6 ms | 94.1 ms | 94.6 ms | 390.9 µs | 0.4% |
| amber-go | `catalog_list` | 2 | 7.2 ms | 7.3 ms | 7.2 ms | 7.3 ms | 23.7 µs | 0.3% |
| amber-go | `read_closure` | 2 | 66.9 ms | 67.8 ms | 66.9 ms | 67.8 ms | 669.1 µs | 1.0% |
| amber-go | `materialize_closure` | 2 | 71.3 ms | 71.8 ms | 71.3 ms | 71.8 ms | 322.4 µs | 0.5% |
| amber-go | `delete_path` | 2 | 6.9 ms | 7.2 ms | 6.9 ms | 7.2 ms | 187.7 µs | 2.7% |
| amber-go | `gc` | 2 | 7.9 ms | 8.4 ms | 7.9 ms | 8.4 ms | 336.3 µs | 4.1% |
| nix | `import_initial` | 2 | 27.1 ms | 28.2 ms | 27.1 ms | 28.2 ms | 796.1 µs | 2.9% |
| nix | `import_repeat` | 2 | 20.5 ms | 21.0 ms | 20.5 ms | 21.0 ms | 346.6 µs | 1.7% |
| nix | `import_incremental` | 2 | 22.2 ms | 22.5 ms | 22.2 ms | 22.5 ms | 265.4 µs | 1.2% |
| nix | `catalog_list` | 2 | 15.2 ms | 16.0 ms | 15.2 ms | 16.0 ms | 575.8 µs | 3.7% |
| nix | `closure_query` | 2 | 15.6 ms | 16.0 ms | 15.6 ms | 16.0 ms | 280.3 µs | 1.8% |
| nix | `read_closure` | 2 | 131.3 ms | 136.1 ms | 131.3 ms | 136.1 ms | 3.4 ms | 2.5% |
| nix | `cache_export` | 2 | 314.5 ms | 314.8 ms | 314.5 ms | 314.8 ms | 174.9 µs | 0.1% |
| nix | `materialize_closure` | 2 | 22.4 ms | 22.5 ms | 22.4 ms | 22.5 ms | 58.2 µs | 0.3% |
| nix | `cache_substitute` | 2 | 22.6 ms | 22.9 ms | 22.6 ms | 22.9 ms | 202.7 µs | 0.9% |
| nix | `delete_path` | 2 | 18.4 ms | 20.7 ms | 18.4 ms | 20.7 ms | 1.6 ms | 8.1% |
| nix | `gc` | 2 | 19.2 ms | 19.6 ms | 19.2 ms | 19.6 ms | 303.6 µs | 1.6% |
| amber-rust | `import_initial` | 2 | 55.4 ms | 57.3 ms | 55.4 ms | 57.3 ms | 1.4 ms | 2.4% |
| amber-rust | `import_repeat` | 2 | 58.3 ms | 58.6 ms | 58.3 ms | 58.6 ms | 224.4 µs | 0.4% |
| amber-rust | `import_incremental` | 2 | 62.7 ms | 67.1 ms | 62.7 ms | 67.1 ms | 3.1 ms | 4.8% |
| amber-rust | `catalog_list` | 2 | 5.0 ms | 5.1 ms | 5.0 ms | 5.1 ms | 48.4 µs | 1.0% |
| amber-rust | `read_closure` | 2 | 47.1 ms | 48.8 ms | 47.1 ms | 48.8 ms | 1.2 ms | 2.6% |
| amber-rust | `materialize_closure` | 2 | 47.5 ms | 55.9 ms | 47.5 ms | 55.9 ms | 6.0 ms | 11.5% |
| amber-rust | `delete_path` | 2 | 5.9 ms | 6.3 ms | 5.9 ms | 6.3 ms | 258.1 µs | 4.2% |
| amber-rust | `gc` | 2 | 5.1 ms | 6.0 ms | 5.1 ms | 6.0 ms | 641.3 µs | 11.5% |

### What these numbers mean, operation by operation

Read these before comparing two cells above: they state what each measured operation actually delivered.

| backend | operation | note |
|---|---|---|
| amber-go | `import_incremental` | one reference per store path: these cores have no notion of a closure, so the set is the harness's bookkeeping |
| amber-go | `import_initial` | one reference per store path: these cores have no notion of a closure, so the set is the harness's bookkeeping |
| amber-go | `import_repeat` | one reference per store path: these cores have no notion of a closure, so the set is the harness's bookkeeping |
| amber-rust | `import_incremental` | one reference per store path: these cores have no notion of a closure, so the set is the harness's bookkeeping |
| amber-rust | `import_initial` | one reference per store path: these cores have no notion of a closure, so the set is the harness's bookkeeping |
| amber-rust | `import_repeat` | one reference per store path: these cores have no notion of a closure, so the set is the harness's bookkeeping |

### Recorded counters (median per operation)

| backend | operation | counter | median |
|---|---|---|---|
| amber-go | `import_initial` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `import_initial` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `import_initial` | `store_packstore_allocated_bytes` | 3.27 MiB |
| amber-go | `import_initial` | `store_packstore_apparent_bytes` | 3.26 MiB |
| amber-go | `import_initial` | `store_refs_allocated_bytes` | 4.45 MiB |
| amber-go | `import_initial` | `store_refs_apparent_bytes` | 9.91 KiB |
| amber-go | `import_repeat` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `import_repeat` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `import_repeat` | `store_packstore_allocated_bytes` | 3.27 MiB |
| amber-go | `import_repeat` | `store_packstore_apparent_bytes` | 3.26 MiB |
| amber-go | `import_repeat` | `store_refs_allocated_bytes` | 4.45 MiB |
| amber-go | `import_repeat` | `store_refs_apparent_bytes` | 10.58 KiB |
| amber-go | `import_incremental` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `import_incremental` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `import_incremental` | `store_packstore_allocated_bytes` | 3.77 MiB |
| amber-go | `import_incremental` | `store_packstore_apparent_bytes` | 3.76 MiB |
| amber-go | `import_incremental` | `store_refs_allocated_bytes` | 4.44 MiB |
| amber-go | `import_incremental` | `store_refs_apparent_bytes` | 9.63 KiB |
| amber-go | `delete_path` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `delete_path` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `delete_path` | `store_packstore_allocated_bytes` | 3.77 MiB |
| amber-go | `delete_path` | `store_packstore_apparent_bytes` | 3.76 MiB |
| amber-go | `delete_path` | `store_refs_allocated_bytes` | 4.45 MiB |
| amber-go | `delete_path` | `store_refs_apparent_bytes` | 10.68 KiB |
| amber-go | `gc` | `allocated_before_delete` | 4009984 |
| amber-go | `gc` | `reclaimed_packstore_bytes` | -4.00 KiB |
| amber-go | `gc` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `gc` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `gc` | `store_packstore_allocated_bytes` | 3.77 MiB |
| amber-go | `gc` | `store_packstore_apparent_bytes` | 3.77 MiB |
| amber-go | `gc` | `store_refs_allocated_bytes` | 28.00 KiB |
| amber-go | `gc` | `store_refs_apparent_bytes` | 7.57 KiB |
| nix | `gc` | `allocated_before_delete` | 17125376 |
| amber-rust | `import_initial` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `import_initial` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `import_initial` | `store_packstore_allocated_bytes` | 3.27 MiB |
| amber-rust | `import_initial` | `store_packstore_apparent_bytes` | 3.26 MiB |
| amber-rust | `import_initial` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `import_initial` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `import_repeat` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `import_repeat` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `import_repeat` | `store_packstore_allocated_bytes` | 3.27 MiB |
| amber-rust | `import_repeat` | `store_packstore_apparent_bytes` | 3.26 MiB |
| amber-rust | `import_repeat` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `import_repeat` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `import_incremental` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `import_incremental` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `import_incremental` | `store_packstore_allocated_bytes` | 3.77 MiB |
| amber-rust | `import_incremental` | `store_packstore_apparent_bytes` | 3.76 MiB |
| amber-rust | `import_incremental` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `import_incremental` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `delete_path` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `delete_path` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `delete_path` | `store_packstore_allocated_bytes` | 3.77 MiB |
| amber-rust | `delete_path` | `store_packstore_apparent_bytes` | 3.76 MiB |
| amber-rust | `delete_path` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `delete_path` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `gc` | `allocated_before_delete` | 5595136 |
| amber-rust | `gc` | `reclaimed_packstore_bytes` | -4.00 KiB |
| amber-rust | `gc` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `gc` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `gc` | `store_packstore_allocated_bytes` | 3.77 MiB |
| amber-rust | `gc` | `store_packstore_apparent_bytes` | 3.77 MiB |
| amber-rust | `gc` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `gc` | `store_refs_apparent_bytes` | 3.52 MiB |

## Scenario `blob/source-history`

### Elapsed time (median, lower is better)

| operation | amber-go-s3 | amber-rust-s3 | git-bundle-s3 |
|---|---|---|---|
| `push_initial` | 43.9 ms | 51.7 ms | 113.1 ms |
| `push_noop` | 18.3 ms | 18.8 ms | 17.2 ms |
| `pull_fresh` | 25.0 ms | 22.4 ms | 48.3 ms |
| `push_incremental` | 42.0 ms | 52.4 ms | 27.8 ms |
| `pull_incremental` | 24.4 ms | 23.6 ms | 70.0 ms |
| `retention_cleanup` | 45.6 ms | 50.2 ms | 153.2 ms |

### Throughput over the logical bytes processed (median)

| operation | amber-go-s3 | amber-rust-s3 | git-bundle-s3 |
|---|---|---|---|
| `push_initial` | 51.45 MiB/s | 107.97 MiB/s | 19.89 MiB/s |
| `push_noop` | 389.36 KiB/s | 134.14 KiB/s | 124.84 KiB/s |
| `pull_fresh` | 35.42 MiB/s | 251.74 MiB/s | 45.25 MiB/s |
| `push_incremental` | 51.73 MiB/s | 108.66 MiB/s | 720.14 KiB/s |
| `pull_incremental` | 92.27 MiB/s | 89.50 MiB/s | 340.17 KiB/s |
| `retention_cleanup` | 52.12 MiB/s | 108.98 MiB/s | 14.69 MiB/s |

### CPU time, user + system (median)

| operation | amber-go-s3 | amber-rust-s3 | git-bundle-s3 |
|---|---|---|---|
| `push_initial` | 43.8 ms | 34.6 ms | 116.6 ms |
| `push_noop` | 24.3 ms | 25.6 ms | 23.8 ms |
| `pull_fresh` | 43.3 ms | 28.9 ms | 53.3 ms |
| `push_incremental` | 42.7 ms | 34.5 ms | 34.5 ms |
| `pull_incremental` | 39.0 ms | 32.4 ms | 36.0 ms |
| `retention_cleanup` | 58.2 ms | 33.6 ms | 163.7 ms |

### Peak resident set size (median)

| operation | amber-go-s3 | amber-rust-s3 | git-bundle-s3 |
|---|---|---|---|
| `push_initial` | 38.58 MiB | 38.58 MiB | 38.66 MiB |
| `push_noop` | 38.58 MiB | 38.58 MiB | 38.66 MiB |
| `pull_fresh` | 38.58 MiB | 38.58 MiB | 38.66 MiB |
| `push_incremental` | 38.58 MiB | 38.58 MiB | 38.66 MiB |
| `pull_incremental` | 38.58 MiB | 38.58 MiB | 38.66 MiB |
| `retention_cleanup` | 38.58 MiB | 38.58 MiB | 38.66 MiB |

### Dispersion of elapsed time

| backend | operation | n | median | p95 | min | max | stddev | cv |
|---|---|---|---|---|---|---|---|---|
| amber-go-s3 | `push_initial` | 2 | 43.9 ms | 45.1 ms | 43.9 ms | 45.1 ms | 836.7 µs | 1.9% |
| amber-go-s3 | `push_noop` | 2 | 18.3 ms | 18.6 ms | 18.3 ms | 18.6 ms | 201.3 µs | 1.1% |
| amber-go-s3 | `pull_fresh` | 2 | 25.0 ms | 65.5 ms | 25.0 ms | 65.5 ms | 28.6 ms | 63.3% |
| amber-go-s3 | `push_incremental` | 2 | 42.0 ms | 45.2 ms | 42.0 ms | 45.2 ms | 2.3 ms | 5.2% |
| amber-go-s3 | `pull_incremental` | 2 | 24.4 ms | 25.2 ms | 24.4 ms | 25.2 ms | 585.5 µs | 2.4% |
| amber-go-s3 | `retention_cleanup` | 2 | 45.6 ms | 45.8 ms | 45.6 ms | 45.8 ms | 130.5 µs | 0.3% |
| amber-rust-s3 | `push_initial` | 2 | 51.7 ms | 53.8 ms | 51.7 ms | 53.8 ms | 1.5 ms | 2.8% |
| amber-rust-s3 | `push_noop` | 2 | 18.8 ms | 19.4 ms | 18.8 ms | 19.4 ms | 392.9 µs | 2.1% |
| amber-rust-s3 | `pull_fresh` | 2 | 22.4 ms | 23.1 ms | 22.4 ms | 23.1 ms | 483.9 µs | 2.1% |
| amber-rust-s3 | `push_incremental` | 2 | 52.4 ms | 53.7 ms | 52.4 ms | 53.7 ms | 918.3 µs | 1.7% |
| amber-rust-s3 | `pull_incremental` | 2 | 23.6 ms | 65.1 ms | 23.6 ms | 65.1 ms | 29.3 ms | 66.2% |
| amber-rust-s3 | `retention_cleanup` | 2 | 50.2 ms | 53.7 ms | 50.2 ms | 53.7 ms | 2.5 ms | 4.8% |
| git-bundle-s3 | `push_initial` | 2 | 113.1 ms | 113.3 ms | 113.1 ms | 113.3 ms | 135.6 µs | 0.1% |
| git-bundle-s3 | `push_noop` | 2 | 17.2 ms | 18.3 ms | 17.2 ms | 18.3 ms | 757.8 µs | 4.3% |
| git-bundle-s3 | `pull_fresh` | 2 | 48.3 ms | 49.8 ms | 48.3 ms | 49.8 ms | 1.1 ms | 2.2% |
| git-bundle-s3 | `push_incremental` | 2 | 27.8 ms | 28.5 ms | 27.8 ms | 28.5 ms | 463.4 µs | 1.6% |
| git-bundle-s3 | `pull_incremental` | 2 | 70.0 ms | 72.3 ms | 70.0 ms | 72.3 ms | 1.6 ms | 2.3% |
| git-bundle-s3 | `retention_cleanup` | 2 | 153.2 ms | 154.5 ms | 153.2 ms | 154.5 ms | 927.7 µs | 0.6% |

### What these numbers mean, operation by operation

Read these before comparing two cells above: they state what each measured operation actually delivered.

| backend | operation | note |
|---|---|---|
| amber-go-s3 | `pull_fresh` | delivered to the destination: the whole published store, which holds all 11 reference(s) published so far; these cores have no selective fetch |
| amber-go-s3 | `pull_fresh` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `pull_incremental` | delivered to the destination: the store's new pack segments and reference database, bringing the client up to the 4 reference(s) the incremental publication added |
| amber-go-s3 | `pull_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `push_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `push_initial` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `push_noop` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `retention_cleanup` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `retention_cleanup` | retained by this retention step: the 4 reference(s) of the incremental publication; the 11 of the initial one were dropped and collected. Retention is the one operation in this scenario where the backends are not asked for the same thing — consolidating Git bundles still publishes the whole history, while dropping an Amber reference, forgetting a restic snapshot or expiring a narinfo removes a version — so these timings are not a like-for-like ranking. What each one kept is stated here, and every member of it is verified afterwards. |
| amber-rust-s3 | `pull_fresh` | delivered to the destination: the whole published store, which holds all 11 reference(s) published so far; these cores have no selective fetch |
| amber-rust-s3 | `pull_fresh` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `pull_incremental` | delivered to the destination: the store's new pack segments and reference database, bringing the client up to the 4 reference(s) the incremental publication added |
| amber-rust-s3 | `pull_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `push_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `push_initial` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `push_noop` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `retention_cleanup` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `retention_cleanup` | retained by this retention step: the 4 reference(s) of the incremental publication; the 11 of the initial one were dropped and collected. Retention is the one operation in this scenario where the backends are not asked for the same thing — consolidating Git bundles still publishes the whole history, while dropping an Amber reference, forgetting a restic snapshot or expiring a narinfo removes a version — so these timings are not a like-for-like ranking. What each one kept is stated here, and every member of it is verified afterwards. |
| git-bundle-s3 | `pull_fresh` | delivered to the destination: the whole published history: 11 commits, every branch and tag of them |
| git-bundle-s3 | `pull_fresh` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| git-bundle-s3 | `pull_incremental` | delivered to the destination: the 4 commits added to main since the first publication, into a clone that already held the rest |
| git-bundle-s3 | `pull_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| git-bundle-s3 | `push_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| git-bundle-s3 | `push_initial` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| git-bundle-s3 | `push_noop` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| git-bundle-s3 | `retention_cleanup` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| git-bundle-s3 | `retention_cleanup` | retained by this retention step: the whole history: 15 commits with every branch and tag, in one consolidated bundle. Consolidating bundles drops no version — what was superseded is the two bundle objects, not any commit. Retention is the one operation in this scenario where the backends are not asked for the same thing — consolidating Git bundles still publishes the whole history, while dropping an Amber reference, forgetting a restic snapshot or expiring a narinfo removes a version — so these timings are not a like-for-like ranking. What each one kept is stated here, and every member of it is verified afterwards. |

### Recorded counters (median per operation)

| backend | operation | counter | median |
|---|---|---|---|
| amber-go-s3 | `push_initial` | `s3_bytes_down` | 4.36 KiB |
| amber-go-s3 | `push_initial` | `s3_bytes_up` | 2.31 MiB |
| amber-go-s3 | `push_initial` | `s3_connections` | 11 |
| amber-go-s3 | `push_initial` | `s3_request_body_bytes` | 2.30 MiB |
| amber-go-s3 | `push_initial` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_initial` | `s3_requests_PUT` | 18 |
| amber-go-s3 | `push_initial` | `s3_requests_total` | 20 |
| amber-go-s3 | `push_initial` | `s3_response_body_bytes` | 617 B |
| amber-go-s3 | `push_initial` | `s3_responses_2xx` | 19 |
| amber-go-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_initial` | `stored_bytes` | 2.30 MiB |
| amber-go-s3 | `push_initial` | `stored_objects` | 18 |
| amber-go-s3 | `push_noop` | `s3_bytes_down` | 6.07 KiB |
| amber-go-s3 | `push_noop` | `s3_bytes_up` | 1.17 KiB |
| amber-go-s3 | `push_noop` | `s3_connections` | 1 |
| amber-go-s3 | `push_noop` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `push_noop` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_noop` | `s3_requests_total` | 2 |
| amber-go-s3 | `push_noop` | `s3_response_body_bytes` | 5.85 KiB |
| amber-go-s3 | `push_noop` | `s3_responses_2xx` | 1 |
| amber-go-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_noop` | `stored_bytes` | 2.30 MiB |
| amber-go-s3 | `push_noop` | `stored_objects` | 18 |
| amber-go-s3 | `pull_fresh` | `delivered_logical_bytes` | 21.48 MiB |
| amber-go-s3 | `pull_fresh` | `delivered_references` | 11 |
| amber-go-s3 | `pull_fresh` | `s3_bytes_down` | 2.31 MiB |
| amber-go-s3 | `pull_fresh` | `s3_bytes_up` | 13.90 KiB |
| amber-go-s3 | `pull_fresh` | `s3_connections` | 13 |
| amber-go-s3 | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `pull_fresh` | `s3_requests_GET` | 22 |
| amber-go-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| amber-go-s3 | `pull_fresh` | `s3_requests_total` | 23 |
| amber-go-s3 | `pull_fresh` | `s3_response_body_bytes` | 2.30 MiB |
| amber-go-s3 | `pull_fresh` | `s3_responses_2xx` | 21 |
| amber-go-s3 | `pull_fresh` | `s3_responses_4xx` | 2 |
| amber-go-s3 | `pull_fresh` | `stored_bytes` | 2.30 MiB |
| amber-go-s3 | `pull_fresh` | `stored_objects` | 18 |
| amber-go-s3 | `push_incremental` | `s3_bytes_down` | 10.89 KiB |
| amber-go-s3 | `push_incremental` | `s3_bytes_up` | 2.33 MiB |
| amber-go-s3 | `push_incremental` | `s3_connections` | 9 |
| amber-go-s3 | `push_incremental` | `s3_request_body_bytes` | 2.31 MiB |
| amber-go-s3 | `push_incremental` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_incremental` | `s3_requests_POST` | 5 |
| amber-go-s3 | `push_incremental` | `s3_requests_PUT` | 11 |
| amber-go-s3 | `push_incremental` | `s3_requests_total` | 18 |
| amber-go-s3 | `push_incremental` | `s3_response_body_bytes` | 7.98 KiB |
| amber-go-s3 | `push_incremental` | `s3_responses_2xx` | 17 |
| amber-go-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_incremental` | `stored_bytes` | 2.31 MiB |
| amber-go-s3 | `push_incremental` | `stored_objects` | 22 |
| amber-go-s3 | `pull_incremental` | `delivered_logical_bytes` | 7.81 MiB |
| amber-go-s3 | `pull_incremental` | `delivered_references` | 4 |
| amber-go-s3 | `pull_incremental` | `s3_bytes_down` | 2.32 MiB |
| amber-go-s3 | `pull_incremental` | `s3_bytes_up` | 9.77 KiB |
| amber-go-s3 | `pull_incremental` | `s3_connections` | 9 |
| amber-go-s3 | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `pull_incremental` | `s3_requests_GET` | 15 |
| amber-go-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| amber-go-s3 | `pull_incremental` | `s3_requests_total` | 16 |
| amber-go-s3 | `pull_incremental` | `s3_response_body_bytes` | 2.32 MiB |
| amber-go-s3 | `pull_incremental` | `s3_responses_2xx` | 14 |
| amber-go-s3 | `pull_incremental` | `s3_responses_4xx` | 2 |
| amber-go-s3 | `pull_incremental` | `stored_bytes` | 2.31 MiB |
| amber-go-s3 | `pull_incremental` | `stored_objects` | 22 |
| amber-go-s3 | `retention_cleanup` | `retained_logical_bytes` | 7.81 MiB |
| amber-go-s3 | `retention_cleanup` | `retained_references` | 4 |
| amber-go-s3 | `retention_cleanup` | `s3_bytes_down` | 21.28 KiB |
| amber-go-s3 | `retention_cleanup` | `s3_bytes_up` | 2.36 MiB |
| amber-go-s3 | `retention_cleanup` | `s3_connections` | 15 |
| amber-go-s3 | `retention_cleanup` | `s3_request_body_bytes` | 2.34 MiB |
| amber-go-s3 | `retention_cleanup` | `s3_requests_GET` | 2 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_POST` | 20 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_PUT` | 18 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_total` | 40 |
| amber-go-s3 | `retention_cleanup` | `s3_response_body_bytes` | 15.42 KiB |
| amber-go-s3 | `retention_cleanup` | `s3_responses_2xx` | 39 |
| amber-go-s3 | `retention_cleanup` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `retention_cleanup` | `stored_bytes` | 2.33 MiB |
| amber-go-s3 | `retention_cleanup` | `stored_objects` | 19 |
| amber-rust-s3 | `push_initial` | `s3_bytes_down` | 1.22 KiB |
| amber-rust-s3 | `push_initial` | `s3_bytes_up` | 5.81 MiB |
| amber-rust-s3 | `push_initial` | `s3_connections` | 2 |
| amber-rust-s3 | `push_initial` | `s3_request_body_bytes` | 5.81 MiB |
| amber-rust-s3 | `push_initial` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_initial` | `s3_requests_PUT` | 2 |
| amber-rust-s3 | `push_initial` | `s3_requests_total` | 4 |
| amber-rust-s3 | `push_initial` | `s3_response_body_bytes` | 619 B |
| amber-rust-s3 | `push_initial` | `s3_responses_2xx` | 3 |
| amber-rust-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_initial` | `stored_bytes` | 5.80 MiB |
| amber-rust-s3 | `push_initial` | `stored_objects` | 2 |
| amber-rust-s3 | `push_noop` | `s3_bytes_down` | 1.43 KiB |
| amber-rust-s3 | `push_noop` | `s3_bytes_up` | 1.17 KiB |
| amber-rust-s3 | `push_noop` | `s3_connections` | 1 |
| amber-rust-s3 | `push_noop` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `push_noop` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_noop` | `s3_requests_total` | 2 |
| amber-rust-s3 | `push_noop` | `s3_response_body_bytes` | 1.21 KiB |
| amber-rust-s3 | `push_noop` | `s3_responses_2xx` | 1 |
| amber-rust-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_noop` | `stored_bytes` | 5.80 MiB |
| amber-rust-s3 | `push_noop` | `stored_objects` | 2 |
| amber-rust-s3 | `pull_fresh` | `delivered_logical_bytes` | 21.48 MiB |
| amber-rust-s3 | `pull_fresh` | `delivered_references` | 11 |
| amber-rust-s3 | `pull_fresh` | `s3_bytes_down` | 5.80 MiB |
| amber-rust-s3 | `pull_fresh` | `s3_bytes_up` | 4.46 KiB |
| amber-rust-s3 | `pull_fresh` | `s3_connections` | 2 |
| amber-rust-s3 | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `pull_fresh` | `s3_requests_GET` | 6 |
| amber-rust-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| amber-rust-s3 | `pull_fresh` | `s3_requests_total` | 7 |
| amber-rust-s3 | `pull_fresh` | `s3_response_body_bytes` | 5.80 MiB |
| amber-rust-s3 | `pull_fresh` | `s3_responses_2xx` | 5 |
| amber-rust-s3 | `pull_fresh` | `s3_responses_4xx` | 2 |
| amber-rust-s3 | `pull_fresh` | `stored_bytes` | 5.80 MiB |
| amber-rust-s3 | `pull_fresh` | `stored_objects` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_bytes_down` | 1.82 KiB |
| amber-rust-s3 | `push_incremental` | `s3_bytes_up` | 5.83 MiB |
| amber-rust-s3 | `push_incremental` | `s3_connections` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_request_body_bytes` | 5.83 MiB |
| amber-rust-s3 | `push_incremental` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_requests_PUT` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_requests_total` | 4 |
| amber-rust-s3 | `push_incremental` | `s3_response_body_bytes` | 1.21 KiB |
| amber-rust-s3 | `push_incremental` | `s3_responses_2xx` | 3 |
| amber-rust-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_incremental` | `stored_bytes` | 5.82 MiB |
| amber-rust-s3 | `push_incremental` | `stored_objects` | 2 |
| amber-rust-s3 | `pull_incremental` | `delivered_logical_bytes` | 7.81 MiB |
| amber-rust-s3 | `pull_incremental` | `delivered_references` | 4 |
| amber-rust-s3 | `pull_incremental` | `s3_bytes_down` | 5.82 MiB |
| amber-rust-s3 | `pull_incremental` | `s3_bytes_up` | 4.46 KiB |
| amber-rust-s3 | `pull_incremental` | `s3_connections` | 2 |
| amber-rust-s3 | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `pull_incremental` | `s3_requests_GET` | 6 |
| amber-rust-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| amber-rust-s3 | `pull_incremental` | `s3_requests_total` | 7 |
| amber-rust-s3 | `pull_incremental` | `s3_response_body_bytes` | 5.82 MiB |
| amber-rust-s3 | `pull_incremental` | `s3_responses_2xx` | 5 |
| amber-rust-s3 | `pull_incremental` | `s3_responses_4xx` | 2 |
| amber-rust-s3 | `pull_incremental` | `stored_bytes` | 5.82 MiB |
| amber-rust-s3 | `pull_incremental` | `stored_objects` | 2 |
| amber-rust-s3 | `retention_cleanup` | `retained_logical_bytes` | 7.81 MiB |
| amber-rust-s3 | `retention_cleanup` | `retained_references` | 4 |
| amber-rust-s3 | `retention_cleanup` | `s3_bytes_down` | 2.37 KiB |
| amber-rust-s3 | `retention_cleanup` | `s3_bytes_up` | 5.85 MiB |
| amber-rust-s3 | `retention_cleanup` | `s3_connections` | 3 |
| amber-rust-s3 | `retention_cleanup` | `s3_request_body_bytes` | 5.84 MiB |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_POST` | 1 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_PUT` | 2 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_total` | 5 |
| amber-rust-s3 | `retention_cleanup` | `s3_response_body_bytes` | 1.65 KiB |
| amber-rust-s3 | `retention_cleanup` | `s3_responses_2xx` | 4 |
| amber-rust-s3 | `retention_cleanup` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `retention_cleanup` | `stored_bytes` | 5.84 MiB |
| amber-rust-s3 | `retention_cleanup` | `stored_objects` | 2 |
| git-bundle-s3 | `push_initial` | `s3_bytes_down` | 1.02 KiB |
| git-bundle-s3 | `push_initial` | `s3_bytes_up` | 2.25 MiB |
| git-bundle-s3 | `push_initial` | `s3_connections` | 1 |
| git-bundle-s3 | `push_initial` | `s3_request_body_bytes` | 2.25 MiB |
| git-bundle-s3 | `push_initial` | `s3_requests_GET` | 2 |
| git-bundle-s3 | `push_initial` | `s3_requests_PUT` | 1 |
| git-bundle-s3 | `push_initial` | `s3_requests_total` | 3 |
| git-bundle-s3 | `push_initial` | `s3_response_body_bytes` | 621 B |
| git-bundle-s3 | `push_initial` | `s3_responses_2xx` | 2 |
| git-bundle-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| git-bundle-s3 | `push_initial` | `stored_bytes` | 2.25 MiB |
| git-bundle-s3 | `push_initial` | `stored_objects` | 1 |
| git-bundle-s3 | `push_noop` | `s3_bytes_down` | 1.12 KiB |
| git-bundle-s3 | `push_noop` | `s3_bytes_up` | 1.17 KiB |
| git-bundle-s3 | `push_noop` | `s3_connections` | 1 |
| git-bundle-s3 | `push_noop` | `s3_request_body_bytes` | 0 B |
| git-bundle-s3 | `push_noop` | `s3_requests_GET` | 2 |
| git-bundle-s3 | `push_noop` | `s3_requests_total` | 2 |
| git-bundle-s3 | `push_noop` | `s3_response_body_bytes` | 917 B |
| git-bundle-s3 | `push_noop` | `s3_responses_2xx` | 1 |
| git-bundle-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| git-bundle-s3 | `push_noop` | `stored_bytes` | 2.25 MiB |
| git-bundle-s3 | `push_noop` | `stored_objects` | 1 |
| git-bundle-s3 | `pull_fresh` | `delivered_logical_bytes` | 21.48 MiB |
| git-bundle-s3 | `pull_fresh` | `delivered_references` | 11 |
| git-bundle-s3 | `pull_fresh` | `s3_bytes_down` | 2.25 MiB |
| git-bundle-s3 | `pull_fresh` | `s3_bytes_up` | 3.02 KiB |
| git-bundle-s3 | `pull_fresh` | `s3_connections` | 1 |
| git-bundle-s3 | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| git-bundle-s3 | `pull_fresh` | `s3_requests_GET` | 4 |
| git-bundle-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| git-bundle-s3 | `pull_fresh` | `s3_requests_total` | 5 |
| git-bundle-s3 | `pull_fresh` | `s3_response_body_bytes` | 2.25 MiB |
| git-bundle-s3 | `pull_fresh` | `s3_responses_2xx` | 3 |
| git-bundle-s3 | `pull_fresh` | `s3_responses_4xx` | 2 |
| git-bundle-s3 | `pull_fresh` | `stored_bytes` | 2.25 MiB |
| git-bundle-s3 | `pull_fresh` | `stored_objects` | 1 |
| git-bundle-s3 | `push_incremental` | `s3_bytes_down` | 1.31 KiB |
| git-bundle-s3 | `push_incremental` | `s3_bytes_up` | 19.20 KiB |
| git-bundle-s3 | `push_incremental` | `s3_connections` | 1 |
| git-bundle-s3 | `push_incremental` | `s3_request_body_bytes` | 17.34 KiB |
| git-bundle-s3 | `push_incremental` | `s3_requests_GET` | 2 |
| git-bundle-s3 | `push_incremental` | `s3_requests_PUT` | 1 |
| git-bundle-s3 | `push_incremental` | `s3_requests_total` | 3 |
| git-bundle-s3 | `push_incremental` | `s3_response_body_bytes` | 917 B |
| git-bundle-s3 | `push_incremental` | `s3_responses_2xx` | 2 |
| git-bundle-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| git-bundle-s3 | `push_incremental` | `stored_bytes` | 2.27 MiB |
| git-bundle-s3 | `push_incremental` | `stored_objects` | 2 |
| git-bundle-s3 | `pull_incremental` | `delivered_logical_bytes` | 7.81 MiB |
| git-bundle-s3 | `pull_incremental` | `delivered_references` | 4 |
| git-bundle-s3 | `pull_incremental` | `s3_bytes_down` | 20.77 KiB |
| git-bundle-s3 | `pull_incremental` | `s3_bytes_up` | 3.84 KiB |
| git-bundle-s3 | `pull_incremental` | `s3_connections` | 1 |
| git-bundle-s3 | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| git-bundle-s3 | `pull_incremental` | `s3_requests_GET` | 5 |
| git-bundle-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| git-bundle-s3 | `pull_incremental` | `s3_requests_total` | 6 |
| git-bundle-s3 | `pull_incremental` | `s3_response_body_bytes` | 20.01 KiB |
| git-bundle-s3 | `pull_incremental` | `s3_responses_2xx` | 4 |
| git-bundle-s3 | `pull_incremental` | `s3_responses_4xx` | 2 |
| git-bundle-s3 | `pull_incremental` | `stored_bytes` | 2.27 MiB |
| git-bundle-s3 | `pull_incremental` | `stored_objects` | 2 |
| git-bundle-s3 | `retention_cleanup` | `retained_logical_bytes` | 29.30 MiB |
| git-bundle-s3 | `retention_cleanup` | `retained_references` | 15 |
| git-bundle-s3 | `retention_cleanup` | `s3_bytes_down` | 3.71 KiB |
| git-bundle-s3 | `retention_cleanup` | `s3_bytes_up` | 2.27 MiB |
| git-bundle-s3 | `retention_cleanup` | `s3_connections` | 3 |
| git-bundle-s3 | `retention_cleanup` | `s3_request_body_bytes` | 2.26 MiB |
| git-bundle-s3 | `retention_cleanup` | `s3_requests_GET` | 4 |
| git-bundle-s3 | `retention_cleanup` | `s3_requests_HEAD` | 4 |
| git-bundle-s3 | `retention_cleanup` | `s3_requests_POST` | 2 |
| git-bundle-s3 | `retention_cleanup` | `s3_requests_PUT` | 1 |
| git-bundle-s3 | `retention_cleanup` | `s3_requests_total` | 11 |
| git-bundle-s3 | `retention_cleanup` | `s3_response_body_bytes` | 1.95 KiB |
| git-bundle-s3 | `retention_cleanup` | `s3_responses_2xx` | 8 |
| git-bundle-s3 | `retention_cleanup` | `s3_responses_4xx` | 3 |
| git-bundle-s3 | `retention_cleanup` | `stored_bytes` | 2.26 MiB |
| git-bundle-s3 | `retention_cleanup` | `stored_objects` | 1 |

## Scenario `tree/lifecycle`

### Elapsed time (median, lower is better)

| operation | restic | fs-sha256 | amber-go | git | amber-rust |
|---|---|---|---|---|---|
| `ingest_fresh` | 791.0 ms | 62.6 ms | 62.1 ms | 233.4 ms | 25.2 ms |
| `ingest_repeat` | 721.2 ms | 21.0 ms | 36.1 ms | 60.3 ms | 17.0 ms |
| `ingest_changed` | 738.3 ms | 27.4 ms | 48.6 ms | 62.0 ms | 17.2 ms |
| `list_recursive` | 724.4 ms | 824.9 µs | 187.5 ms | 3.6 ms | 138.5 ms |
| `read_full` | 748.4 ms | 3.8 ms | 24.6 ms | 61.1 ms | 12.1 ms |
| `restore_full` | 766.4 ms | 15.4 ms | 51.0 ms | 71.3 ms | 46.5 ms |
| `ref_delete` | 716.4 ms | 1.4 ms | 13.7 ms | 2.1 ms | 9.4 ms |
| `gc` | 887.4 ms | 2.9 ms | 14.8 ms | 303.4 ms | 11.3 ms |

### Throughput over the logical bytes processed (median)

| operation | restic | fs-sha256 | amber-go | git | amber-rust |
|---|---|---|---|---|---|
| `ingest_fresh` | 40.95 MiB/s | 518.54 MiB/s | 473.32 MiB/s | 138.57 MiB/s | 1.21 GiB/s |
| `ingest_repeat` | 45.17 MiB/s | 1.45 GiB/s | 715.73 MiB/s | 537.84 MiB/s | 1.61 GiB/s |
| `ingest_changed` | 44.05 MiB/s | 1.16 GiB/s | 637.29 MiB/s | 514.56 MiB/s | 1.68 GiB/s |
| `list_recursive` | — | — | — | — | — |
| `read_full` | 43.19 MiB/s | 7.75 GiB/s | 1.28 GiB/s | 531.08 MiB/s | 2.55 GiB/s |
| `restore_full` | 42.50 MiB/s | 1.98 GiB/s | 639.66 MiB/s | 453.56 MiB/s | 696.86 MiB/s |
| `ref_delete` | — | — | — | — | — |
| `gc` | — | — | — | — | — |

### CPU time, user + system (median)

| operation | restic | fs-sha256 | amber-go | git | amber-rust |
|---|---|---|---|---|---|
| `ingest_fresh` | 920.8 ms | 43.0 ms | 513.1 ms | 231.6 ms | 150.2 ms |
| `ingest_repeat` | 534.0 ms | 20.6 ms | 317.3 ms | 59.6 ms | 83.9 ms |
| `ingest_changed` | 708.6 ms | 23.3 ms | 408.4 ms | 61.1 ms | 86.9 ms |
| `list_recursive` | 534.5 ms | 765.0 µs | 238.3 ms | 3.5 ms | 127.8 ms |
| `read_full` | 575.5 ms | 3.7 ms | 36.7 ms | 60.7 ms | 11.7 ms |
| `restore_full` | 578.1 ms | 15.3 ms | 70.8 ms | 70.8 ms | 45.7 ms |
| `ref_delete` | 523.4 ms | 1.2 ms | 17.8 ms | 1.9 ms | 8.5 ms |
| `gc` | 713.9 ms | 2.8 ms | 30.0 ms | 384.7 ms | 17.2 ms |

### Peak resident set size (median)

| operation | restic | fs-sha256 | amber-go | git | amber-rust |
|---|---|---|---|---|---|
| `ingest_fresh` | 233.54 MiB | 38.58 MiB | 235.39 MiB | 38.66 MiB | 48.14 MiB |
| `ingest_repeat` | 67.98 MiB | 38.58 MiB | 120.34 MiB | 38.66 MiB | 39.00 MiB |
| `ingest_changed` | 120.36 MiB | 38.58 MiB | 200.09 MiB | 38.66 MiB | 39.00 MiB |
| `list_recursive` | 67.21 MiB | 38.58 MiB | 38.66 MiB | 38.66 MiB | 39.00 MiB |
| `read_full` | 87.88 MiB | 38.58 MiB | 44.40 MiB | 38.66 MiB | 39.00 MiB |
| `restore_full` | 83.51 MiB | 38.58 MiB | 41.46 MiB | 38.66 MiB | 39.00 MiB |
| `ref_delete` | 66.84 MiB | 38.58 MiB | 38.66 MiB | 39.00 MiB | 39.00 MiB |
| `gc` | 163.84 MiB | 38.58 MiB | 38.66 MiB | 50.25 MiB | 39.00 MiB |

### Storage allocated after each operation (median)

| operation | restic | fs-sha256 | amber-go | git | amber-rust |
|---|---|---|---|---|---|
| `ingest_fresh` | 14.10 MiB | 25.70 MiB | 15.64 MiB | 21.86 MiB | 12.77 MiB |
| `ingest_repeat` | 14.11 MiB | 25.74 MiB | 15.65 MiB | 21.87 MiB | 12.77 MiB |
| `ingest_changed` | 14.32 MiB | 26.11 MiB | 15.84 MiB | 22.36 MiB | 12.96 MiB |
| `list_recursive` | — | — | — | — | — |
| `read_full` | — | — | — | — | — |
| `restore_full` | — | — | — | — | — |
| `ref_delete` | 14.31 MiB | 26.02 MiB | 15.84 MiB | 22.34 MiB | 12.96 MiB |
| `gc` | 14.12 MiB | 25.77 MiB | 11.29 MiB | 11.21 MiB | 12.78 MiB |

### Space reclaimed (median; positive means released)

| operation | restic | fs-sha256 | amber-go | git | amber-rust |
|---|---|---|---|---|---|
| `ingest_fresh` | — | — | — | — | — |
| `ingest_repeat` | — | — | — | — | — |
| `ingest_changed` | — | — | — | — | — |
| `list_recursive` | — | — | — | — | — |
| `read_full` | — | — | — | — | — |
| `restore_full` | — | — | — | — | — |
| `ref_delete` | — | — | — | — | — |
| `gc` | 188.00 KiB | 252.00 KiB | 4.53 MiB | 11.13 MiB | 168.00 KiB |

### Dispersion of elapsed time

| backend | operation | n | median | p95 | min | max | stddev | cv |
|---|---|---|---|---|---|---|---|---|
| restic | `ingest_fresh` | 2 | 791.0 ms | 797.4 ms | 791.0 ms | 797.4 ms | 4.5 ms | 0.6% |
| restic | `ingest_repeat` | 2 | 721.2 ms | 722.8 ms | 721.2 ms | 722.8 ms | 1.2 ms | 0.2% |
| restic | `ingest_changed` | 2 | 738.3 ms | 741.7 ms | 738.3 ms | 741.7 ms | 2.4 ms | 0.3% |
| restic | `list_recursive` | 2 | 724.4 ms | 724.6 ms | 724.4 ms | 724.6 ms | 92.9 µs | 0.0% |
| restic | `read_full` | 2 | 748.4 ms | 755.9 ms | 748.4 ms | 755.9 ms | 5.3 ms | 0.7% |
| restic | `restore_full` | 2 | 766.4 ms | 768.2 ms | 766.4 ms | 768.2 ms | 1.3 ms | 0.2% |
| restic | `ref_delete` | 2 | 716.4 ms | 737.6 ms | 716.4 ms | 737.6 ms | 15.0 ms | 2.1% |
| restic | `gc` | 2 | 887.4 ms | 902.3 ms | 887.4 ms | 902.3 ms | 10.6 ms | 1.2% |
| fs-sha256 | `ingest_fresh` | 2 | 62.6 ms | 63.0 ms | 62.6 ms | 63.0 ms | 288.8 µs | 0.5% |
| fs-sha256 | `ingest_repeat` | 2 | 21.0 ms | 22.1 ms | 21.0 ms | 22.1 ms | 762.8 µs | 3.5% |
| fs-sha256 | `ingest_changed` | 2 | 27.4 ms | 27.5 ms | 27.4 ms | 27.5 ms | 95.5 µs | 0.3% |
| fs-sha256 | `list_recursive` | 2 | 824.9 µs | 977.8 µs | 824.9 µs | 977.8 µs | 108.1 µs | 12.0% |
| fs-sha256 | `read_full` | 2 | 3.8 ms | 4.1 ms | 3.8 ms | 4.1 ms | 255.1 µs | 6.5% |
| fs-sha256 | `restore_full` | 2 | 15.4 ms | 16.1 ms | 15.4 ms | 16.1 ms | 475.5 µs | 3.0% |
| fs-sha256 | `ref_delete` | 2 | 1.4 ms | 1.5 ms | 1.4 ms | 1.5 ms | 52.0 µs | 3.7% |
| fs-sha256 | `gc` | 2 | 2.9 ms | 3.0 ms | 2.9 ms | 3.0 ms | 45.5 µs | 1.5% |
| amber-go | `ingest_fresh` | 2 | 62.1 ms | 69.0 ms | 62.1 ms | 69.0 ms | 4.9 ms | 7.4% |
| amber-go | `ingest_repeat` | 2 | 36.1 ms | 45.6 ms | 36.1 ms | 45.6 ms | 6.7 ms | 16.4% |
| amber-go | `ingest_changed` | 2 | 48.6 ms | 51.3 ms | 48.6 ms | 51.3 ms | 1.9 ms | 3.8% |
| amber-go | `list_recursive` | 2 | 187.5 ms | 187.6 ms | 187.5 ms | 187.6 ms | 38.8 µs | 0.0% |
| amber-go | `read_full` | 2 | 24.6 ms | 24.8 ms | 24.6 ms | 24.8 ms | 167.0 µs | 0.7% |
| amber-go | `restore_full` | 2 | 51.0 ms | 51.0 ms | 51.0 ms | 51.0 ms | 34.4 µs | 0.1% |
| amber-go | `ref_delete` | 2 | 13.7 ms | 14.5 ms | 13.7 ms | 14.5 ms | 512.3 µs | 3.6% |
| amber-go | `gc` | 2 | 14.8 ms | 16.9 ms | 14.8 ms | 16.9 ms | 1.5 ms | 9.7% |
| git | `ingest_fresh` | 2 | 233.4 ms | 235.6 ms | 233.4 ms | 235.6 ms | 1.5 ms | 0.7% |
| git | `ingest_repeat` | 2 | 60.3 ms | 60.7 ms | 60.3 ms | 60.7 ms | 260.1 µs | 0.4% |
| git | `ingest_changed` | 2 | 62.0 ms | 63.5 ms | 62.0 ms | 63.5 ms | 1.0 ms | 1.7% |
| git | `list_recursive` | 2 | 3.6 ms | 3.9 ms | 3.6 ms | 3.9 ms | 215.9 µs | 5.7% |
| git | `read_full` | 2 | 61.1 ms | 61.5 ms | 61.1 ms | 61.5 ms | 270.5 µs | 0.4% |
| git | `restore_full` | 2 | 71.3 ms | 72.0 ms | 71.3 ms | 72.0 ms | 506.9 µs | 0.7% |
| git | `ref_delete` | 2 | 2.1 ms | 2.2 ms | 2.1 ms | 2.2 ms | 106.0 µs | 4.9% |
| git | `gc` | 2 | 303.4 ms | 305.8 ms | 303.4 ms | 305.8 ms | 1.7 ms | 0.5% |
| amber-rust | `ingest_fresh` | 2 | 25.2 ms | 26.4 ms | 25.2 ms | 26.4 ms | 888.2 µs | 3.4% |
| amber-rust | `ingest_repeat` | 2 | 17.0 ms | 19.7 ms | 17.0 ms | 19.7 ms | 1.9 ms | 10.6% |
| amber-rust | `ingest_changed` | 2 | 17.2 ms | 18.9 ms | 17.2 ms | 18.9 ms | 1.2 ms | 6.7% |
| amber-rust | `list_recursive` | 2 | 138.5 ms | 142.0 ms | 138.5 ms | 142.0 ms | 2.5 ms | 1.7% |
| amber-rust | `read_full` | 2 | 12.1 ms | 12.5 ms | 12.1 ms | 12.5 ms | 288.3 µs | 2.3% |
| amber-rust | `restore_full` | 2 | 46.5 ms | 46.9 ms | 46.5 ms | 46.9 ms | 267.0 µs | 0.6% |
| amber-rust | `ref_delete` | 2 | 9.4 ms | 11.9 ms | 9.4 ms | 11.9 ms | 1.7 ms | 16.0% |
| amber-rust | `gc` | 2 | 11.3 ms | 11.9 ms | 11.3 ms | 11.9 ms | 432.9 µs | 3.7% |

### What these numbers mean, operation by operation

Read these before comparing two cells above: they state what each measured operation actually delivered.

| backend | operation | note |
|---|---|---|
| amber-go | `gc` | reclaimed_packstore_bytes is the pack-segment bytes the collection released; the whole-store reclaim figure also includes the reference database, which compacts itself and is not comparable between the two cores |
| amber-go | `ingest_repeat` | the store already holds every byte; what is measured is how cheaply the backend recognises that |
| amber-go | `list_recursive` | neither Amber CLI has a recursive listing, so this is 28 separate `amber-store ls --keys` processes, one per directory; the number therefore includes 28 process startups and is not comparable with a single-process listing without allowing for them |
| amber-rust | `gc` | reclaimed_packstore_bytes is the pack-segment bytes the collection released; the whole-store reclaim figure also includes the reference database, which compacts itself and is not comparable between the two cores |
| amber-rust | `ingest_repeat` | the store already holds every byte; what is measured is how cheaply the backend recognises that |
| amber-rust | `list_recursive` | neither Amber CLI has a recursive listing, so this is 28 separate `amber-store ls --keys` processes, one per directory; the number therefore includes 28 process startups and is not comparable with a single-process listing without allowing for them |
| fs-sha256 | `ingest_repeat` | the store already holds every byte; what is measured is how cheaply the backend recognises that |
| git | `ingest_repeat` | the store already holds every byte; what is measured is how cheaply the backend recognises that |
| restic | `ingest_repeat` | the store already holds every byte; what is measured is how cheaply the backend recognises that |

### Recorded counters (median per operation)

| backend | operation | counter | median |
|---|---|---|---|
| restic | `ingest_fresh` | `store_data_allocated_bytes` | 14.05 MiB |
| restic | `ingest_fresh` | `store_data_apparent_bytes` | 13.05 MiB |
| restic | `ingest_fresh` | `store_index_allocated_bytes` | 20.00 KiB |
| restic | `ingest_fresh` | `store_index_apparent_bytes` | 12.53 KiB |
| restic | `ingest_fresh` | `store_snapshots_allocated_bytes` | 8.00 KiB |
| restic | `ingest_fresh` | `store_snapshots_apparent_bytes` | 405 B |
| restic | `ingest_repeat` | `store_data_allocated_bytes` | 14.05 MiB |
| restic | `ingest_repeat` | `store_data_apparent_bytes` | 13.05 MiB |
| restic | `ingest_repeat` | `store_index_allocated_bytes` | 20.00 KiB |
| restic | `ingest_repeat` | `store_index_apparent_bytes` | 12.53 KiB |
| restic | `ingest_repeat` | `store_snapshots_allocated_bytes` | 12.00 KiB |
| restic | `ingest_repeat` | `store_snapshots_apparent_bytes` | 853 B |
| restic | `ingest_changed` | `store_data_allocated_bytes` | 14.26 MiB |
| restic | `ingest_changed` | `store_data_apparent_bytes` | 13.25 MiB |
| restic | `ingest_changed` | `store_index_allocated_bytes` | 24.00 KiB |
| restic | `ingest_changed` | `store_index_apparent_bytes` | 16.31 KiB |
| restic | `ingest_changed` | `store_snapshots_allocated_bytes` | 16.00 KiB |
| restic | `ingest_changed` | `store_snapshots_apparent_bytes` | 1.23 KiB |
| restic | `ref_delete` | `store_data_allocated_bytes` | 14.26 MiB |
| restic | `ref_delete` | `store_data_apparent_bytes` | 13.25 MiB |
| restic | `ref_delete` | `store_index_allocated_bytes` | 24.00 KiB |
| restic | `ref_delete` | `store_index_apparent_bytes` | 16.31 KiB |
| restic | `ref_delete` | `store_snapshots_allocated_bytes` | 8.00 KiB |
| restic | `ref_delete` | `store_snapshots_apparent_bytes` | 402 B |
| restic | `gc` | `allocated_after_gc` | 14811136 |
| restic | `gc` | `allocated_before_delete` | 15011840 |
| restic | `gc` | `allocated_before_gc` | 15003648 |
| restic | `gc` | `store_data_allocated_bytes` | 14.08 MiB |
| restic | `gc` | `store_data_apparent_bytes` | 13.07 MiB |
| restic | `gc` | `store_index_allocated_bytes` | 20.00 KiB |
| restic | `gc` | `store_index_apparent_bytes` | 12.59 KiB |
| restic | `gc` | `store_snapshots_allocated_bytes` | 8.00 KiB |
| restic | `gc` | `store_snapshots_apparent_bytes` | 402 B |
| fs-sha256 | `ingest_fresh` | `store_refs_allocated_bytes` | 48.00 KiB |
| fs-sha256 | `ingest_fresh` | `store_refs_apparent_bytes` | 41.75 KiB |
| fs-sha256 | `ingest_repeat` | `store_refs_allocated_bytes` | 92.00 KiB |
| fs-sha256 | `ingest_repeat` | `store_refs_apparent_bytes` | 83.49 KiB |
| fs-sha256 | `ingest_changed` | `store_refs_allocated_bytes` | 136.00 KiB |
| fs-sha256 | `ingest_changed` | `store_refs_apparent_bytes` | 125.31 KiB |
| fs-sha256 | `ref_delete` | `store_refs_allocated_bytes` | 48.00 KiB |
| fs-sha256 | `ref_delete` | `store_refs_apparent_bytes` | 41.82 KiB |
| fs-sha256 | `gc` | `allocated_after_gc` | 27025408 |
| fs-sha256 | `gc` | `allocated_before_delete` | 27373568 |
| fs-sha256 | `gc` | `allocated_before_gc` | 27283456 |
| fs-sha256 | `gc` | `store_refs_allocated_bytes` | 48.00 KiB |
| fs-sha256 | `gc` | `store_refs_apparent_bytes` | 41.82 KiB |
| amber-go | `ingest_fresh` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `ingest_fresh` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `ingest_fresh` | `store_packstore_allocated_bytes` | 11.22 MiB |
| amber-go | `ingest_fresh` | `store_packstore_apparent_bytes` | 11.21 MiB |
| amber-go | `ingest_fresh` | `store_refs_allocated_bytes` | 4.41 MiB |
| amber-go | `ingest_fresh` | `store_refs_apparent_bytes` | 2.85 KiB |
| amber-go | `ingest_repeat` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `ingest_repeat` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `ingest_repeat` | `store_packstore_allocated_bytes` | 11.22 MiB |
| amber-go | `ingest_repeat` | `store_packstore_apparent_bytes` | 11.21 MiB |
| amber-go | `ingest_repeat` | `store_refs_allocated_bytes` | 4.42 MiB |
| amber-go | `ingest_repeat` | `store_refs_apparent_bytes` | 3.55 KiB |
| amber-go | `ingest_changed` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `ingest_changed` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `ingest_changed` | `store_packstore_allocated_bytes` | 11.41 MiB |
| amber-go | `ingest_changed` | `store_packstore_apparent_bytes` | 11.40 MiB |
| amber-go | `ingest_changed` | `store_refs_allocated_bytes` | 4.43 MiB |
| amber-go | `ingest_changed` | `store_refs_apparent_bytes` | 4.26 KiB |
| amber-go | `list_recursive` | `cli_invocations` | 28 |
| amber-go | `list_recursive` | `directories` | 27 |
| amber-go | `list_recursive` | `entries` | 256 |
| amber-go | `ref_delete` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `ref_delete` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `ref_delete` | `store_packstore_allocated_bytes` | 11.41 MiB |
| amber-go | `ref_delete` | `store_packstore_apparent_bytes` | 11.40 MiB |
| amber-go | `ref_delete` | `store_refs_allocated_bytes` | 4.42 MiB |
| amber-go | `ref_delete` | `store_refs_apparent_bytes` | 3.93 KiB |
| amber-go | `gc` | `allocated_after_gc` | 11837440 |
| amber-go | `gc` | `allocated_before_delete` | 12001280 |
| amber-go | `gc` | `allocated_before_gc` | 16605184 |
| amber-go | `gc` | `reclaimed_packstore_bytes` | 140.00 KiB |
| amber-go | `gc` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `gc` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `gc` | `store_packstore_allocated_bytes` | 11.25 MiB |
| amber-go | `gc` | `store_packstore_apparent_bytes` | 11.25 MiB |
| amber-go | `gc` | `store_refs_allocated_bytes` | 28.00 KiB |
| amber-go | `gc` | `store_refs_apparent_bytes` | 4.46 KiB |
| git | `ingest_fresh` | `store_refs_allocated_bytes` | 16.00 KiB |
| git | `ingest_fresh` | `store_refs_apparent_bytes` | 41 B |
| git | `ingest_repeat` | `store_refs_allocated_bytes` | 20.00 KiB |
| git | `ingest_repeat` | `store_refs_apparent_bytes` | 82 B |
| git | `ingest_changed` | `store_refs_allocated_bytes` | 24.00 KiB |
| git | `ingest_changed` | `store_refs_apparent_bytes` | 123 B |
| git | `ref_delete` | `store_refs_allocated_bytes` | 16.00 KiB |
| git | `ref_delete` | `store_refs_apparent_bytes` | 41 B |
| git | `gc` | `allocated_after_gc` | 11755520 |
| git | `gc` | `allocated_before_delete` | 23441408 |
| git | `gc` | `allocated_before_gc` | 23425024 |
| git | `gc` | `store_refs_allocated_bytes` | 12.00 KiB |
| git | `gc` | `store_refs_apparent_bytes` | 0 B |
| amber-rust | `ingest_fresh` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `ingest_fresh` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `ingest_fresh` | `store_packstore_allocated_bytes` | 11.21 MiB |
| amber-rust | `ingest_fresh` | `store_packstore_apparent_bytes` | 11.20 MiB |
| amber-rust | `ingest_fresh` | `store_refs_allocated_bytes` | 1.55 MiB |
| amber-rust | `ingest_fresh` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `ingest_repeat` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `ingest_repeat` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `ingest_repeat` | `store_packstore_allocated_bytes` | 11.21 MiB |
| amber-rust | `ingest_repeat` | `store_packstore_apparent_bytes` | 11.20 MiB |
| amber-rust | `ingest_repeat` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `ingest_repeat` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `ingest_changed` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `ingest_changed` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `ingest_changed` | `store_packstore_allocated_bytes` | 11.39 MiB |
| amber-rust | `ingest_changed` | `store_packstore_apparent_bytes` | 11.39 MiB |
| amber-rust | `ingest_changed` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `ingest_changed` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `list_recursive` | `cli_invocations` | 28 |
| amber-rust | `list_recursive` | `directories` | 27 |
| amber-rust | `list_recursive` | `entries` | 256 |
| amber-rust | `ref_delete` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `ref_delete` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `ref_delete` | `store_packstore_allocated_bytes` | 11.39 MiB |
| amber-rust | `ref_delete` | `store_packstore_apparent_bytes` | 11.39 MiB |
| amber-rust | `ref_delete` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `ref_delete` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `gc` | `allocated_after_gc` | 13402112 |
| amber-rust | `gc` | `allocated_before_delete` | 13590528 |
| amber-rust | `gc` | `allocated_before_gc` | 13590528 |
| amber-rust | `gc` | `reclaimed_packstore_bytes` | 168.00 KiB |
| amber-rust | `gc` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `gc` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `gc` | `store_packstore_allocated_bytes` | 11.21 MiB |
| amber-rust | `gc` | `store_packstore_apparent_bytes` | 11.21 MiB |
| amber-rust | `gc` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `gc` | `store_refs_apparent_bytes` | 3.52 MiB |

## Scenario `backup/retention`

### Elapsed time (median, lower is better)

| operation | amber-rust | amber-go | restic |
|---|---|---|---|
| `backup_initial` | 24.2 ms | 62.9 ms | 778.2 ms |
| `backup_unchanged` | 17.2 ms | 33.8 ms | 724.9 ms |
| `backup_changed` | 16.1 ms | 52.5 ms | 735.7 ms |
| `backup_changed_again` | 18.4 ms | 48.8 ms | 740.6 ms |
| `list_snapshot` | 136.9 ms | 200.7 ms | 721.4 ms |
| `restore_latest` | 46.1 ms | 51.3 ms | 768.3 ms |
| `forget_oldest` | 11.0 ms | 14.0 ms | 717.5 ms |
| `prune` | 12.3 ms | 15.0 ms | 889.1 ms |

### Throughput over the logical bytes processed (median)

| operation | amber-rust | amber-go | restic |
|---|---|---|---|
| `backup_initial` | 1.27 GiB/s | 470.84 MiB/s | 41.35 MiB/s |
| `backup_unchanged` | 1.85 GiB/s | 811.38 MiB/s | 44.85 MiB/s |
| `backup_changed` | 1.73 GiB/s | 613.86 MiB/s | 44.19 MiB/s |
| `backup_changed_again` | 1.67 GiB/s | 627.73 MiB/s | 44.03 MiB/s |
| `list_snapshot` | — | — | — |
| `restore_latest` | 700.35 MiB/s | 621.54 MiB/s | 42.16 MiB/s |
| `forget_oldest` | — | — | — |
| `prune` | — | — | — |

### CPU time, user + system (median)

| operation | amber-rust | amber-go | restic |
|---|---|---|---|
| `backup_initial` | 151.7 ms | 468.4 ms | 920.5 ms |
| `backup_unchanged` | 82.7 ms | 312.0 ms | 539.2 ms |
| `backup_changed` | 83.4 ms | 500.1 ms | 679.6 ms |
| `backup_changed_again` | 85.4 ms | 360.9 ms | 706.3 ms |
| `list_snapshot` | 125.8 ms | 253.4 ms | 532.0 ms |
| `restore_latest` | 45.3 ms | 73.3 ms | 582.8 ms |
| `forget_oldest` | 10.1 ms | 18.1 ms | 527.9 ms |
| `prune` | 18.2 ms | 31.4 ms | 716.3 ms |

### Peak resident set size (median)

| operation | amber-rust | amber-go | restic |
|---|---|---|---|
| `backup_initial` | 49.38 MiB | 209.58 MiB | 234.02 MiB |
| `backup_unchanged` | 38.58 MiB | 123.79 MiB | 67.15 MiB |
| `backup_changed` | 38.58 MiB | 188.54 MiB | 114.39 MiB |
| `backup_changed_again` | 38.58 MiB | 188.83 MiB | 122.40 MiB |
| `list_snapshot` | 38.58 MiB | 38.66 MiB | 66.39 MiB |
| `restore_latest` | 38.58 MiB | 41.64 MiB | 79.40 MiB |
| `forget_oldest` | 38.58 MiB | 38.66 MiB | 65.66 MiB |
| `prune` | 38.58 MiB | 38.66 MiB | 164.57 MiB |

### Storage allocated after each operation (median)

| operation | amber-rust | amber-go | restic |
|---|---|---|---|
| `backup_initial` | 12.76 MiB | 15.64 MiB | 13.59 MiB |
| `backup_unchanged` | 12.77 MiB | 15.64 MiB | 13.60 MiB |
| `backup_changed` | 12.96 MiB | 15.84 MiB | 13.81 MiB |
| `backup_changed_again` | 13.14 MiB | 16.03 MiB | 14.02 MiB |
| `list_snapshot` | — | — | — |
| `restore_latest` | — | — | — |
| `forget_oldest` | 13.14 MiB | 16.02 MiB | 14.01 MiB |
| `prune` | 12.97 MiB | 11.48 MiB | 13.83 MiB |

### Space reclaimed (median; positive means released)

| operation | amber-rust | amber-go | restic |
|---|---|---|---|
| `backup_initial` | — | — | — |
| `backup_unchanged` | — | — | — |
| `backup_changed` | — | — | — |
| `backup_changed_again` | — | — | — |
| `list_snapshot` | — | — | — |
| `restore_latest` | — | — | — |
| `forget_oldest` | — | — | — |
| `prune` | 180.00 KiB | 4.52 MiB | 184.00 KiB |

### Dispersion of elapsed time

| backend | operation | n | median | p95 | min | max | stddev | cv |
|---|---|---|---|---|---|---|---|---|
| amber-rust | `backup_initial` | 2 | 24.2 ms | 25.0 ms | 24.2 ms | 25.0 ms | 579.4 µs | 2.4% |
| amber-rust | `backup_unchanged` | 2 | 17.2 ms | 17.2 ms | 17.2 ms | 17.2 ms | 44.0 µs | 0.3% |
| amber-rust | `backup_changed` | 2 | 16.1 ms | 18.4 ms | 16.1 ms | 18.4 ms | 1.6 ms | 9.4% |
| amber-rust | `backup_changed_again` | 2 | 18.4 ms | 19.1 ms | 18.4 ms | 19.1 ms | 529.8 µs | 2.8% |
| amber-rust | `list_snapshot` | 2 | 136.9 ms | 159.1 ms | 136.9 ms | 159.1 ms | 15.7 ms | 10.6% |
| amber-rust | `restore_latest` | 2 | 46.1 ms | 46.7 ms | 46.1 ms | 46.7 ms | 383.9 µs | 0.8% |
| amber-rust | `forget_oldest` | 2 | 11.0 ms | 12.0 ms | 11.0 ms | 12.0 ms | 657.9 µs | 5.7% |
| amber-rust | `prune` | 2 | 12.3 ms | 12.7 ms | 12.3 ms | 12.7 ms | 280.2 µs | 2.2% |
| amber-go | `backup_initial` | 2 | 62.9 ms | 69.3 ms | 62.9 ms | 69.3 ms | 4.6 ms | 6.9% |
| amber-go | `backup_unchanged` | 2 | 33.8 ms | 40.2 ms | 33.8 ms | 40.2 ms | 4.5 ms | 12.3% |
| amber-go | `backup_changed` | 2 | 52.5 ms | 53.2 ms | 52.5 ms | 53.2 ms | 493.0 µs | 0.9% |
| amber-go | `backup_changed_again` | 2 | 48.8 ms | 52.1 ms | 48.8 ms | 52.1 ms | 2.3 ms | 4.6% |
| amber-go | `list_snapshot` | 2 | 200.7 ms | 202.2 ms | 200.7 ms | 202.2 ms | 1.1 ms | 0.5% |
| amber-go | `restore_latest` | 2 | 51.3 ms | 52.6 ms | 51.3 ms | 52.6 ms | 925.6 µs | 1.8% |
| amber-go | `forget_oldest` | 2 | 14.0 ms | 15.2 ms | 14.0 ms | 15.2 ms | 865.4 µs | 5.9% |
| amber-go | `prune` | 2 | 15.0 ms | 16.1 ms | 15.0 ms | 16.1 ms | 745.9 µs | 4.8% |
| restic | `backup_initial` | 2 | 778.2 ms | 789.6 ms | 778.2 ms | 789.6 ms | 8.1 ms | 1.0% |
| restic | `backup_unchanged` | 2 | 724.9 ms | 728.1 ms | 724.9 ms | 728.1 ms | 2.2 ms | 0.3% |
| restic | `backup_changed` | 2 | 735.7 ms | 739.3 ms | 735.7 ms | 739.3 ms | 2.5 ms | 0.3% |
| restic | `backup_changed_again` | 2 | 740.6 ms | 742.4 ms | 740.6 ms | 742.4 ms | 1.2 ms | 0.2% |
| restic | `list_snapshot` | 2 | 721.4 ms | 732.1 ms | 721.4 ms | 732.1 ms | 7.6 ms | 1.0% |
| restic | `restore_latest` | 2 | 768.3 ms | 775.3 ms | 768.3 ms | 775.3 ms | 5.0 ms | 0.6% |
| restic | `forget_oldest` | 2 | 717.5 ms | 719.6 ms | 717.5 ms | 719.6 ms | 1.5 ms | 0.2% |
| restic | `prune` | 2 | 889.1 ms | 904.3 ms | 889.1 ms | 904.3 ms | 10.8 ms | 1.2% |

### What these numbers mean, operation by operation

Read these before comparing two cells above: they state what each measured operation actually delivered.

| backend | operation | note |
|---|---|---|
| amber-go | `list_snapshot` | 29 separate `amber-store ls` processes, one per directory: neither core's CLI has a recursive listing |
| amber-go | `prune` | reclaimed_packstore_bytes is the pack-segment bytes the prune released; the whole-store figure also includes the reference database, which compacts itself |
| amber-rust | `list_snapshot` | 29 separate `amber-store ls` processes, one per directory: neither core's CLI has a recursive listing |
| amber-rust | `prune` | reclaimed_packstore_bytes is the pack-segment bytes the prune released; the whole-store figure also includes the reference database, which compacts itself |

### Recorded counters (median per operation)

| backend | operation | counter | median |
|---|---|---|---|
| amber-rust | `backup_initial` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `backup_initial` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `backup_initial` | `store_packstore_allocated_bytes` | 11.20 MiB |
| amber-rust | `backup_initial` | `store_packstore_apparent_bytes` | 11.20 MiB |
| amber-rust | `backup_initial` | `store_refs_allocated_bytes` | 1.55 MiB |
| amber-rust | `backup_initial` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `backup_unchanged` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `backup_unchanged` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `backup_unchanged` | `store_packstore_allocated_bytes` | 11.20 MiB |
| amber-rust | `backup_unchanged` | `store_packstore_apparent_bytes` | 11.20 MiB |
| amber-rust | `backup_unchanged` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `backup_unchanged` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `backup_changed` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `backup_changed` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `backup_changed` | `store_packstore_allocated_bytes` | 11.39 MiB |
| amber-rust | `backup_changed` | `store_packstore_apparent_bytes` | 11.39 MiB |
| amber-rust | `backup_changed` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `backup_changed` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `backup_changed_again` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `backup_changed_again` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `backup_changed_again` | `store_packstore_allocated_bytes` | 11.58 MiB |
| amber-rust | `backup_changed_again` | `store_packstore_apparent_bytes` | 11.57 MiB |
| amber-rust | `backup_changed_again` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `backup_changed_again` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `list_snapshot` | `cli_invocations` | 29 |
| amber-rust | `list_snapshot` | `entries` | 256 |
| amber-rust | `forget_oldest` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `forget_oldest` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `forget_oldest` | `store_packstore_allocated_bytes` | 11.58 MiB |
| amber-rust | `forget_oldest` | `store_packstore_apparent_bytes` | 11.57 MiB |
| amber-rust | `forget_oldest` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `forget_oldest` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `prune` | `allocated_after_prune` | 13598720 |
| amber-rust | `prune` | `allocated_before_forget` | 13783040 |
| amber-rust | `prune` | `reclaimed_packstore_bytes` | 180.00 KiB |
| amber-rust | `prune` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `prune` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `prune` | `store_packstore_allocated_bytes` | 11.40 MiB |
| amber-rust | `prune` | `store_packstore_apparent_bytes` | 11.40 MiB |
| amber-rust | `prune` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `prune` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-go | `backup_initial` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `backup_initial` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `backup_initial` | `store_packstore_allocated_bytes` | 11.21 MiB |
| amber-go | `backup_initial` | `store_packstore_apparent_bytes` | 11.21 MiB |
| amber-go | `backup_initial` | `store_refs_allocated_bytes` | 4.41 MiB |
| amber-go | `backup_initial` | `store_refs_apparent_bytes` | 2.85 KiB |
| amber-go | `backup_unchanged` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `backup_unchanged` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `backup_unchanged` | `store_packstore_allocated_bytes` | 11.21 MiB |
| amber-go | `backup_unchanged` | `store_packstore_apparent_bytes` | 11.21 MiB |
| amber-go | `backup_unchanged` | `store_refs_allocated_bytes` | 4.42 MiB |
| amber-go | `backup_unchanged` | `store_refs_apparent_bytes` | 3.56 KiB |
| amber-go | `backup_changed` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `backup_changed` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `backup_changed` | `store_packstore_allocated_bytes` | 11.40 MiB |
| amber-go | `backup_changed` | `store_packstore_apparent_bytes` | 11.40 MiB |
| amber-go | `backup_changed` | `store_refs_allocated_bytes` | 4.43 MiB |
| amber-go | `backup_changed` | `store_refs_apparent_bytes` | 4.28 KiB |
| amber-go | `backup_changed_again` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `backup_changed_again` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `backup_changed_again` | `store_packstore_allocated_bytes` | 11.59 MiB |
| amber-go | `backup_changed_again` | `store_packstore_apparent_bytes` | 11.58 MiB |
| amber-go | `backup_changed_again` | `store_refs_allocated_bytes` | 4.43 MiB |
| amber-go | `backup_changed_again` | `store_refs_apparent_bytes` | 4.98 KiB |
| amber-go | `list_snapshot` | `cli_invocations` | 29 |
| amber-go | `list_snapshot` | `entries` | 256 |
| amber-go | `forget_oldest` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `forget_oldest` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `forget_oldest` | `store_packstore_allocated_bytes` | 11.59 MiB |
| amber-go | `forget_oldest` | `store_packstore_apparent_bytes` | 11.58 MiB |
| amber-go | `forget_oldest` | `store_refs_allocated_bytes` | 4.42 MiB |
| amber-go | `forget_oldest` | `store_refs_apparent_bytes` | 4.08 KiB |
| amber-go | `prune` | `allocated_after_prune` | 12042240 |
| amber-go | `prune` | `allocated_before_forget` | 12197888 |
| amber-go | `prune` | `reclaimed_packstore_bytes` | 124.00 KiB |
| amber-go | `prune` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `prune` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `prune` | `store_packstore_allocated_bytes` | 11.45 MiB |
| amber-go | `prune` | `store_packstore_apparent_bytes` | 11.44 MiB |
| amber-go | `prune` | `store_refs_allocated_bytes` | 28.00 KiB |
| amber-go | `prune` | `store_refs_apparent_bytes` | 4.57 KiB |
| restic | `backup_initial` | `store_data_allocated_bytes` | 13.55 MiB |
| restic | `backup_initial` | `store_data_apparent_bytes` | 12.54 MiB |
| restic | `backup_initial` | `store_index_allocated_bytes` | 20.00 KiB |
| restic | `backup_initial` | `store_index_apparent_bytes` | 12.60 KiB |
| restic | `backup_initial` | `store_snapshots_allocated_bytes` | 8.00 KiB |
| restic | `backup_initial` | `store_snapshots_apparent_bytes` | 410 B |
| restic | `backup_unchanged` | `store_data_allocated_bytes` | 13.55 MiB |
| restic | `backup_unchanged` | `store_data_apparent_bytes` | 12.54 MiB |
| restic | `backup_unchanged` | `store_index_allocated_bytes` | 20.00 KiB |
| restic | `backup_unchanged` | `store_index_apparent_bytes` | 12.60 KiB |
| restic | `backup_unchanged` | `store_snapshots_allocated_bytes` | 12.00 KiB |
| restic | `backup_unchanged` | `store_snapshots_apparent_bytes` | 860 B |
| restic | `backup_changed` | `store_data_allocated_bytes` | 13.75 MiB |
| restic | `backup_changed` | `store_data_apparent_bytes` | 12.74 MiB |
| restic | `backup_changed` | `store_index_allocated_bytes` | 24.00 KiB |
| restic | `backup_changed` | `store_index_apparent_bytes` | 16.39 KiB |
| restic | `backup_changed` | `store_snapshots_allocated_bytes` | 16.00 KiB |
| restic | `backup_changed` | `store_snapshots_apparent_bytes` | 1.24 KiB |
| restic | `backup_changed_again` | `store_data_allocated_bytes` | 13.95 MiB |
| restic | `backup_changed_again` | `store_data_apparent_bytes` | 12.94 MiB |
| restic | `backup_changed_again` | `store_index_allocated_bytes` | 28.00 KiB |
| restic | `backup_changed_again` | `store_index_apparent_bytes` | 20.13 KiB |
| restic | `backup_changed_again` | `store_snapshots_allocated_bytes` | 20.00 KiB |
| restic | `backup_changed_again` | `store_snapshots_apparent_bytes` | 1.63 KiB |
| restic | `forget_oldest` | `store_data_allocated_bytes` | 13.95 MiB |
| restic | `forget_oldest` | `store_data_apparent_bytes` | 12.94 MiB |
| restic | `forget_oldest` | `store_index_allocated_bytes` | 28.00 KiB |
| restic | `forget_oldest` | `store_index_apparent_bytes` | 20.13 KiB |
| restic | `forget_oldest` | `store_snapshots_allocated_bytes` | 12.00 KiB |
| restic | `forget_oldest` | `store_snapshots_apparent_bytes` | 813 B |
| restic | `prune` | `allocated_after_prune` | 14503936 |
| restic | `prune` | `allocated_before_forget` | 14700544 |
| restic | `prune` | `store_data_allocated_bytes` | 13.78 MiB |
| restic | `prune` | `store_data_apparent_bytes` | 12.76 MiB |
| restic | `prune` | `store_index_allocated_bytes` | 24.00 KiB |
| restic | `prune` | `store_index_apparent_bytes` | 16.25 KiB |
| restic | `prune` | `store_snapshots_allocated_bytes` | 12.00 KiB |
| restic | `prune` | `store_snapshots_apparent_bytes` | 813 B |

## Scenario `blob/backup-corpus`

### Elapsed time (median, lower is better)

| operation | restic-s3 | amber-rust-s3 | amber-go-s3 |
|---|---|---|---|
| `push_initial` | 924.1 ms | 89.7 ms | 93.7 ms |
| `push_noop` | 728.8 ms | 18.6 ms | 18.0 ms |
| `pull_fresh` | 864.1 ms | 25.7 ms | 27.4 ms |
| `push_incremental` | 751.8 ms | 51.8 ms | 52.7 ms |
| `pull_incremental` | 859.1 ms | 23.6 ms | 23.5 ms |
| `retention_cleanup` | 1.74 s | 91.0 ms | 89.9 ms |

### Throughput over the logical bytes processed (median)

| operation | restic-s3 | amber-rust-s3 | amber-go-s3 |
|---|---|---|---|
| `push_initial` | 13.77 MiB/s | 153.46 MiB/s | 114.51 MiB/s |
| `push_noop` | 19.29 KiB/s | 151.98 KiB/s | 230.06 KiB/s |
| `pull_fresh` | 14.69 MiB/s | 558.89 MiB/s | 408.40 MiB/s |
| `push_incremental` | 298.44 KiB/s | 129.86 MiB/s | 61.81 MiB/s |
| `pull_incremental` | 14.96 MiB/s | 106.68 MiB/s | 50.85 MiB/s |
| `retention_cleanup` | 14.18 MiB/s | 154.81 MiB/s | 118.92 MiB/s |

### CPU time, user + system (median)

| operation | restic-s3 | amber-rust-s3 | amber-go-s3 |
|---|---|---|---|
| `push_initial` | 912.0 ms | 41.0 ms | 44.2 ms |
| `push_noop` | 541.4 ms | 24.4 ms | 25.7 ms |
| `pull_fresh` | 584.6 ms | 32.7 ms | 38.7 ms |
| `push_incremental` | 704.7 ms | 35.7 ms | 39.4 ms |
| `pull_incremental` | 590.4 ms | 29.8 ms | 33.3 ms |
| `retention_cleanup` | 1.26 s | 46.6 ms | 50.1 ms |

### Peak resident set size (median)

| operation | restic-s3 | amber-rust-s3 | amber-go-s3 |
|---|---|---|---|
| `push_initial` | 245.66 MiB | 38.58 MiB | 38.58 MiB |
| `push_noop` | 69.52 MiB | 38.58 MiB | 38.58 MiB |
| `pull_fresh` | 83.01 MiB | 38.58 MiB | 38.58 MiB |
| `push_incremental` | 119.22 MiB | 38.58 MiB | 38.66 MiB |
| `pull_incremental` | 84.12 MiB | 38.58 MiB | 38.66 MiB |
| `retention_cleanup` | 175.61 MiB | 38.58 MiB | 38.66 MiB |

### Dispersion of elapsed time

| backend | operation | n | median | p95 | min | max | stddev | cv |
|---|---|---|---|---|---|---|---|---|
| restic-s3 | `push_initial` | 2 | 924.1 ms | 936.2 ms | 924.1 ms | 936.2 ms | 8.6 ms | 0.9% |
| restic-s3 | `push_noop` | 2 | 728.8 ms | 732.7 ms | 728.8 ms | 732.7 ms | 2.8 ms | 0.4% |
| restic-s3 | `pull_fresh` | 2 | 864.1 ms | 899.6 ms | 864.1 ms | 899.6 ms | 25.1 ms | 2.8% |
| restic-s3 | `push_incremental` | 2 | 751.8 ms | 755.6 ms | 751.8 ms | 755.6 ms | 2.7 ms | 0.4% |
| restic-s3 | `pull_incremental` | 2 | 859.1 ms | 864.9 ms | 859.1 ms | 864.9 ms | 4.1 ms | 0.5% |
| restic-s3 | `retention_cleanup` | 2 | 1.74 s | 1.78 s | 1.74 s | 1.78 s | 27.9 ms | 1.6% |
| amber-rust-s3 | `push_initial` | 2 | 89.7 ms | 96.0 ms | 89.7 ms | 96.0 ms | 4.5 ms | 4.8% |
| amber-rust-s3 | `push_noop` | 2 | 18.6 ms | 19.1 ms | 18.6 ms | 19.1 ms | 351.0 µs | 1.9% |
| amber-rust-s3 | `pull_fresh` | 2 | 25.7 ms | 26.3 ms | 25.7 ms | 26.3 ms | 477.8 µs | 1.8% |
| amber-rust-s3 | `push_incremental` | 2 | 51.8 ms | 52.9 ms | 51.8 ms | 52.9 ms | 738.2 µs | 1.4% |
| amber-rust-s3 | `pull_incremental` | 2 | 23.6 ms | 64.3 ms | 23.6 ms | 64.3 ms | 28.8 ms | 65.4% |
| amber-rust-s3 | `retention_cleanup` | 2 | 91.0 ms | 95.3 ms | 91.0 ms | 95.3 ms | 3.1 ms | 3.3% |
| amber-go-s3 | `push_initial` | 2 | 93.7 ms | 98.1 ms | 93.7 ms | 98.1 ms | 3.1 ms | 3.3% |
| amber-go-s3 | `push_noop` | 2 | 18.0 ms | 19.0 ms | 18.0 ms | 19.0 ms | 666.4 µs | 3.6% |
| amber-go-s3 | `pull_fresh` | 2 | 27.4 ms | 27.5 ms | 27.4 ms | 27.5 ms | 63.8 µs | 0.2% |
| amber-go-s3 | `push_incremental` | 2 | 52.7 ms | 55.1 ms | 52.7 ms | 55.1 ms | 1.7 ms | 3.1% |
| amber-go-s3 | `pull_incremental` | 2 | 23.5 ms | 66.8 ms | 23.5 ms | 66.8 ms | 30.7 ms | 67.9% |
| amber-go-s3 | `retention_cleanup` | 2 | 89.9 ms | 94.8 ms | 89.9 ms | 94.8 ms | 3.5 ms | 3.7% |

### What these numbers mean, operation by operation

Read these before comparing two cells above: they state what each measured operation actually delivered.

| backend | operation | note |
|---|---|---|
| amber-go-s3 | `pull_fresh` | delivered to the destination: the whole published store, which holds all 1 reference(s) published so far; these cores have no selective fetch |
| amber-go-s3 | `pull_fresh` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `pull_incremental` | delivered to the destination: the store's new pack segments and reference database, bringing the client up to the 1 reference(s) the incremental publication added |
| amber-go-s3 | `pull_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `push_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `push_initial` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `push_noop` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `retention_cleanup` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `retention_cleanup` | retained by this retention step: the 1 reference(s) of the incremental publication; the 1 of the initial one were dropped and collected. Retention is the one operation in this scenario where the backends are not asked for the same thing — consolidating Git bundles still publishes the whole history, while dropping an Amber reference, forgetting a restic snapshot or expiring a narinfo removes a version — so these timings are not a like-for-like ranking. What each one kept is stated here, and every member of it is verified afterwards. |
| amber-rust-s3 | `pull_fresh` | delivered to the destination: the whole published store, which holds all 1 reference(s) published so far; these cores have no selective fetch |
| amber-rust-s3 | `pull_fresh` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `pull_incremental` | delivered to the destination: the store's new pack segments and reference database, bringing the client up to the 1 reference(s) the incremental publication added |
| amber-rust-s3 | `pull_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `push_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `push_initial` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `push_noop` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `retention_cleanup` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `retention_cleanup` | retained by this retention step: the 1 reference(s) of the incremental publication; the 1 of the initial one were dropped and collected. Retention is the one operation in this scenario where the backends are not asked for the same thing — consolidating Git bundles still publishes the whole history, while dropping an Amber reference, forgetting a restic snapshot or expiring a narinfo removes a version — so these timings are not a like-for-like ranking. What each one kept is stated here, and every member of it is verified afterwards. |
| restic-s3 | `pull_fresh` | delivered to the destination: snapshot g0 (corpus generation 0). g0-again holds the identical tree, so restoring it as well would deliver the same bytes twice |
| restic-s3 | `pull_fresh` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| restic-s3 | `pull_incremental` | delivered to the destination: snapshot g1 (corpus generation 1), into a client that already held generation 0 |
| restic-s3 | `pull_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| restic-s3 | `push_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| restic-s3 | `push_initial` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| restic-s3 | `push_noop` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| restic-s3 | `retention_cleanup` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| restic-s3 | `retention_cleanup` | retained by this retention step: snapshot g1 (corpus generation 1); both snapshots of generation 0 were forgotten and pruned. Retention is the one operation in this scenario where the backends are not asked for the same thing — consolidating Git bundles still publishes the whole history, while dropping an Amber reference, forgetting a restic snapshot or expiring a narinfo removes a version — so these timings are not a like-for-like ranking. What each one kept is stated here, and every member of it is verified afterwards. |

### Recorded counters (median per operation)

| backend | operation | counter | median |
|---|---|---|---|
| restic-s3 | `push_initial` | `s3_bytes_down` | 5.76 KiB |
| restic-s3 | `push_initial` | `s3_bytes_up` | 12.72 MiB |
| restic-s3 | `push_initial` | `s3_connections` | 1 |
| restic-s3 | `push_initial` | `s3_request_body_bytes` | 12.71 MiB |
| restic-s3 | `push_initial` | `s3_requests_DELETE` | 1 |
| restic-s3 | `push_initial` | `s3_requests_GET` | 8 |
| restic-s3 | `push_initial` | `s3_requests_HEAD` | 1 |
| restic-s3 | `push_initial` | `s3_requests_PUT` | 5 |
| restic-s3 | `push_initial` | `s3_requests_total` | 15 |
| restic-s3 | `push_initial` | `s3_response_body_bytes` | 3.41 KiB |
| restic-s3 | `push_initial` | `s3_responses_2xx` | 14 |
| restic-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| restic-s3 | `push_initial` | `stored_bytes` | 12.69 MiB |
| restic-s3 | `push_initial` | `stored_objects` | 6 |
| restic-s3 | `push_noop` | `s3_bytes_down` | 5.85 KiB |
| restic-s3 | `push_noop` | `s3_bytes_up` | 8.28 KiB |
| restic-s3 | `push_noop` | `s3_connections` | 1 |
| restic-s3 | `push_noop` | `s3_request_body_bytes` | 966 B |
| restic-s3 | `push_noop` | `s3_requests_DELETE` | 1 |
| restic-s3 | `push_noop` | `s3_requests_GET` | 8 |
| restic-s3 | `push_noop` | `s3_requests_HEAD` | 1 |
| restic-s3 | `push_noop` | `s3_requests_PUT` | 2 |
| restic-s3 | `push_noop` | `s3_requests_total` | 12 |
| restic-s3 | `push_noop` | `s3_response_body_bytes` | 4.09 KiB |
| restic-s3 | `push_noop` | `s3_responses_2xx` | 11 |
| restic-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| restic-s3 | `push_noop` | `stored_bytes` | 12.69 MiB |
| restic-s3 | `push_noop` | `stored_objects` | 7 |
| restic-s3 | `pull_fresh` | `delivered_logical_bytes` | 32.65 MiB |
| restic-s3 | `pull_fresh` | `delivered_references` | 1 |
| restic-s3 | `pull_fresh` | `s3_bytes_down` | 12.68 MiB |
| restic-s3 | `pull_fresh` | `s3_bytes_up` | 8.72 KiB |
| restic-s3 | `pull_fresh` | `s3_connections` | 1 |
| restic-s3 | `pull_fresh` | `s3_request_body_bytes` | 345 B |
| restic-s3 | `pull_fresh` | `s3_requests_DELETE` | 1 |
| restic-s3 | `pull_fresh` | `s3_requests_GET` | 11 |
| restic-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| restic-s3 | `pull_fresh` | `s3_requests_PUT` | 1 |
| restic-s3 | `pull_fresh` | `s3_requests_total` | 14 |
| restic-s3 | `pull_fresh` | `s3_response_body_bytes` | 12.68 MiB |
| restic-s3 | `pull_fresh` | `s3_responses_2xx` | 13 |
| restic-s3 | `pull_fresh` | `s3_responses_4xx` | 1 |
| restic-s3 | `pull_fresh` | `stored_bytes` | 12.69 MiB |
| restic-s3 | `pull_fresh` | `stored_objects` | 7 |
| restic-s3 | `push_incremental` | `s3_bytes_down` | 6.78 KiB |
| restic-s3 | `push_incremental` | `s3_bytes_up` | 218.73 KiB |
| restic-s3 | `push_incremental` | `s3_connections` | 2 |
| restic-s3 | `push_incremental` | `s3_request_body_bytes` | 209.16 KiB |
| restic-s3 | `push_incremental` | `s3_requests_DELETE` | 1 |
| restic-s3 | `push_incremental` | `s3_requests_GET` | 8 |
| restic-s3 | `push_incremental` | `s3_requests_HEAD` | 1 |
| restic-s3 | `push_incremental` | `s3_requests_PUT` | 5 |
| restic-s3 | `push_incremental` | `s3_requests_total` | 15 |
| restic-s3 | `push_incremental` | `s3_response_body_bytes` | 4.43 KiB |
| restic-s3 | `push_incremental` | `s3_responses_2xx` | 14 |
| restic-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| restic-s3 | `push_incremental` | `stored_bytes` | 12.89 MiB |
| restic-s3 | `push_incremental` | `stored_objects` | 11 |
| restic-s3 | `pull_incremental` | `delivered_logical_bytes` | 32.67 MiB |
| restic-s3 | `pull_incremental` | `delivered_references` | 1 |
| restic-s3 | `pull_incremental` | `s3_bytes_down` | 12.84 MiB |
| restic-s3 | `pull_incremental` | `s3_bytes_up` | 9.33 KiB |
| restic-s3 | `pull_incremental` | `s3_connections` | 2 |
| restic-s3 | `pull_incremental` | `s3_request_body_bytes` | 345 B |
| restic-s3 | `pull_incremental` | `s3_requests_DELETE` | 1 |
| restic-s3 | `pull_incremental` | `s3_requests_GET` | 12 |
| restic-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| restic-s3 | `pull_incremental` | `s3_requests_PUT` | 1 |
| restic-s3 | `pull_incremental` | `s3_requests_total` | 15 |
| restic-s3 | `pull_incremental` | `s3_response_body_bytes` | 12.84 MiB |
| restic-s3 | `pull_incremental` | `s3_responses_2xx` | 14 |
| restic-s3 | `pull_incremental` | `s3_responses_4xx` | 1 |
| restic-s3 | `pull_incremental` | `stored_bytes` | 12.89 MiB |
| restic-s3 | `pull_incremental` | `stored_objects` | 11 |
| restic-s3 | `retention_cleanup` | `retained_logical_bytes` | 32.67 MiB |
| restic-s3 | `retention_cleanup` | `retained_references` | 1 |
| restic-s3 | `retention_cleanup` | `s3_bytes_down` | 12.65 MiB |
| restic-s3 | `retention_cleanup` | `s3_bytes_up` | 12.56 MiB |
| restic-s3 | `retention_cleanup` | `s3_connections` | 5 |
| restic-s3 | `retention_cleanup` | `s3_request_body_bytes` | 12.54 MiB |
| restic-s3 | `retention_cleanup` | `s3_requests_DELETE` | 9 |
| restic-s3 | `retention_cleanup` | `s3_requests_GET` | 17 |
| restic-s3 | `retention_cleanup` | `s3_requests_HEAD` | 2 |
| restic-s3 | `retention_cleanup` | `s3_requests_PUT` | 6 |
| restic-s3 | `retention_cleanup` | `s3_requests_total` | 34 |
| restic-s3 | `retention_cleanup` | `s3_response_body_bytes` | 12.65 MiB |
| restic-s3 | `retention_cleanup` | `s3_responses_2xx` | 32 |
| restic-s3 | `retention_cleanup` | `s3_responses_4xx` | 2 |
| restic-s3 | `retention_cleanup` | `stored_bytes` | 12.71 MiB |
| restic-s3 | `retention_cleanup` | `stored_objects` | 8 |
| amber-rust-s3 | `push_initial` | `s3_bytes_down` | 1.41 KiB |
| amber-rust-s3 | `push_initial` | `s3_bytes_up` | 14.74 MiB |
| amber-rust-s3 | `push_initial` | `s3_connections` | 3 |
| amber-rust-s3 | `push_initial` | `s3_request_body_bytes` | 14.73 MiB |
| amber-rust-s3 | `push_initial` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_initial` | `s3_requests_PUT` | 3 |
| amber-rust-s3 | `push_initial` | `s3_requests_total` | 5 |
| amber-rust-s3 | `push_initial` | `s3_response_body_bytes` | 618 B |
| amber-rust-s3 | `push_initial` | `s3_responses_2xx` | 4 |
| amber-rust-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_initial` | `stored_bytes` | 14.71 MiB |
| amber-rust-s3 | `push_initial` | `stored_objects` | 3 |
| amber-rust-s3 | `push_noop` | `s3_bytes_down` | 1.74 KiB |
| amber-rust-s3 | `push_noop` | `s3_bytes_up` | 1.17 KiB |
| amber-rust-s3 | `push_noop` | `s3_connections` | 1 |
| amber-rust-s3 | `push_noop` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `push_noop` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_noop` | `s3_requests_total` | 2 |
| amber-rust-s3 | `push_noop` | `s3_response_body_bytes` | 1.51 KiB |
| amber-rust-s3 | `push_noop` | `s3_responses_2xx` | 1 |
| amber-rust-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_noop` | `stored_bytes` | 14.71 MiB |
| amber-rust-s3 | `push_noop` | `stored_objects` | 3 |
| amber-rust-s3 | `pull_fresh` | `delivered_logical_bytes` | 32.65 MiB |
| amber-rust-s3 | `pull_fresh` | `delivered_references` | 1 |
| amber-rust-s3 | `pull_fresh` | `s3_bytes_down` | 14.72 MiB |
| amber-rust-s3 | `pull_fresh` | `s3_bytes_up` | 5.02 KiB |
| amber-rust-s3 | `pull_fresh` | `s3_connections` | 3 |
| amber-rust-s3 | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `pull_fresh` | `s3_requests_GET` | 7 |
| amber-rust-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| amber-rust-s3 | `pull_fresh` | `s3_requests_total` | 8 |
| amber-rust-s3 | `pull_fresh` | `s3_response_body_bytes` | 14.72 MiB |
| amber-rust-s3 | `pull_fresh` | `s3_responses_2xx` | 6 |
| amber-rust-s3 | `pull_fresh` | `s3_responses_4xx` | 2 |
| amber-rust-s3 | `pull_fresh` | `stored_bytes` | 14.71 MiB |
| amber-rust-s3 | `pull_fresh` | `stored_objects` | 3 |
| amber-rust-s3 | `push_incremental` | `s3_bytes_down` | 2.13 KiB |
| amber-rust-s3 | `push_incremental` | `s3_bytes_up` | 6.86 MiB |
| amber-rust-s3 | `push_incremental` | `s3_connections` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_request_body_bytes` | 6.86 MiB |
| amber-rust-s3 | `push_incremental` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_requests_PUT` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_requests_total` | 4 |
| amber-rust-s3 | `push_incremental` | `s3_response_body_bytes` | 1.51 KiB |
| amber-rust-s3 | `push_incremental` | `s3_responses_2xx` | 3 |
| amber-rust-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_incremental` | `stored_bytes` | 14.90 MiB |
| amber-rust-s3 | `push_incremental` | `stored_objects` | 3 |
| amber-rust-s3 | `pull_incremental` | `delivered_logical_bytes` | 32.67 MiB |
| amber-rust-s3 | `pull_incremental` | `delivered_references` | 1 |
| amber-rust-s3 | `pull_incremental` | `s3_bytes_down` | 6.86 MiB |
| amber-rust-s3 | `pull_incremental` | `s3_bytes_up` | 4.42 KiB |
| amber-rust-s3 | `pull_incremental` | `s3_connections` | 2 |
| amber-rust-s3 | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `pull_incremental` | `s3_requests_GET` | 6 |
| amber-rust-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| amber-rust-s3 | `pull_incremental` | `s3_requests_total` | 7 |
| amber-rust-s3 | `pull_incremental` | `s3_response_body_bytes` | 6.85 MiB |
| amber-rust-s3 | `pull_incremental` | `s3_responses_2xx` | 5 |
| amber-rust-s3 | `pull_incremental` | `s3_responses_4xx` | 2 |
| amber-rust-s3 | `pull_incremental` | `stored_bytes` | 14.90 MiB |
| amber-rust-s3 | `pull_incremental` | `stored_objects` | 3 |
| amber-rust-s3 | `retention_cleanup` | `retained_logical_bytes` | 32.67 MiB |
| amber-rust-s3 | `retention_cleanup` | `retained_references` | 1 |
| amber-rust-s3 | `retention_cleanup` | `s3_bytes_down` | 3.41 KiB |
| amber-rust-s3 | `retention_cleanup` | `s3_bytes_up` | 14.75 MiB |
| amber-rust-s3 | `retention_cleanup` | `s3_connections` | 5 |
| amber-rust-s3 | `retention_cleanup` | `s3_request_body_bytes` | 14.74 MiB |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_POST` | 2 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_PUT` | 3 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_total` | 7 |
| amber-rust-s3 | `retention_cleanup` | `s3_response_body_bytes` | 2.39 KiB |
| amber-rust-s3 | `retention_cleanup` | `s3_responses_2xx` | 6 |
| amber-rust-s3 | `retention_cleanup` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `retention_cleanup` | `stored_bytes` | 14.72 MiB |
| amber-rust-s3 | `retention_cleanup` | `stored_objects` | 3 |
| amber-go-s3 | `push_initial` | `s3_bytes_down` | 2.39 KiB |
| amber-go-s3 | `push_initial` | `s3_bytes_up` | 11.24 MiB |
| amber-go-s3 | `push_initial` | `s3_connections` | 7 |
| amber-go-s3 | `push_initial` | `s3_request_body_bytes` | 11.23 MiB |
| amber-go-s3 | `push_initial` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_initial` | `s3_requests_PUT` | 8 |
| amber-go-s3 | `push_initial` | `s3_requests_total` | 10 |
| amber-go-s3 | `push_initial` | `s3_response_body_bytes` | 616 B |
| amber-go-s3 | `push_initial` | `s3_responses_2xx` | 9 |
| amber-go-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_initial` | `stored_bytes` | 11.21 MiB |
| amber-go-s3 | `push_initial` | `stored_objects` | 8 |
| amber-go-s3 | `push_noop` | `s3_bytes_down` | 3.20 KiB |
| amber-go-s3 | `push_noop` | `s3_bytes_up` | 1.17 KiB |
| amber-go-s3 | `push_noop` | `s3_connections` | 1 |
| amber-go-s3 | `push_noop` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `push_noop` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_noop` | `s3_requests_total` | 2 |
| amber-go-s3 | `push_noop` | `s3_response_body_bytes` | 2.98 KiB |
| amber-go-s3 | `push_noop` | `s3_responses_2xx` | 1 |
| amber-go-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_noop` | `stored_bytes` | 11.21 MiB |
| amber-go-s3 | `push_noop` | `stored_objects` | 8 |
| amber-go-s3 | `pull_fresh` | `delivered_logical_bytes` | 32.65 MiB |
| amber-go-s3 | `pull_fresh` | `delivered_references` | 1 |
| amber-go-s3 | `pull_fresh` | `s3_bytes_down` | 11.22 MiB |
| amber-go-s3 | `pull_fresh` | `s3_bytes_up` | 7.99 KiB |
| amber-go-s3 | `pull_fresh` | `s3_connections` | 8 |
| amber-go-s3 | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `pull_fresh` | `s3_requests_GET` | 12 |
| amber-go-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| amber-go-s3 | `pull_fresh` | `s3_requests_total` | 13 |
| amber-go-s3 | `pull_fresh` | `s3_response_body_bytes` | 11.22 MiB |
| amber-go-s3 | `pull_fresh` | `s3_responses_2xx` | 11 |
| amber-go-s3 | `pull_fresh` | `s3_responses_4xx` | 2 |
| amber-go-s3 | `pull_fresh` | `stored_bytes` | 11.21 MiB |
| amber-go-s3 | `pull_fresh` | `stored_objects` | 8 |
| amber-go-s3 | `push_incremental` | `s3_bytes_down` | 6.17 KiB |
| amber-go-s3 | `push_incremental` | `s3_bytes_up` | 3.33 MiB |
| amber-go-s3 | `push_incremental` | `s3_connections` | 9 |
| amber-go-s3 | `push_incremental` | `s3_request_body_bytes` | 3.32 MiB |
| amber-go-s3 | `push_incremental` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_incremental` | `s3_requests_POST` | 3 |
| amber-go-s3 | `push_incremental` | `s3_requests_PUT` | 7 |
| amber-go-s3 | `push_incremental` | `s3_requests_total` | 12 |
| amber-go-s3 | `push_incremental` | `s3_response_body_bytes` | 4.26 KiB |
| amber-go-s3 | `push_incremental` | `s3_responses_2xx` | 11 |
| amber-go-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_incremental` | `stored_bytes` | 11.40 MiB |
| amber-go-s3 | `push_incremental` | `stored_objects` | 10 |
| amber-go-s3 | `pull_incremental` | `delivered_logical_bytes` | 32.67 MiB |
| amber-go-s3 | `pull_incremental` | `delivered_references` | 1 |
| amber-go-s3 | `pull_incremental` | `s3_bytes_down` | 3.32 MiB |
| amber-go-s3 | `pull_incremental` | `s3_bytes_up` | 7.37 KiB |
| amber-go-s3 | `pull_incremental` | `s3_connections` | 7 |
| amber-go-s3 | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `pull_incremental` | `s3_requests_GET` | 11 |
| amber-go-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| amber-go-s3 | `pull_incremental` | `s3_requests_total` | 12 |
| amber-go-s3 | `pull_incremental` | `s3_response_body_bytes` | 3.32 MiB |
| amber-go-s3 | `pull_incremental` | `s3_responses_2xx` | 10 |
| amber-go-s3 | `pull_incremental` | `s3_responses_4xx` | 2 |
| amber-go-s3 | `pull_incremental` | `stored_bytes` | 11.40 MiB |
| amber-go-s3 | `pull_incremental` | `stored_objects` | 10 |
| amber-go-s3 | `retention_cleanup` | `retained_logical_bytes` | 32.67 MiB |
| amber-go-s3 | `retention_cleanup` | `retained_references` | 1 |
| amber-go-s3 | `retention_cleanup` | `s3_bytes_down` | 9.80 KiB |
| amber-go-s3 | `retention_cleanup` | `s3_bytes_up` | 11.26 MiB |
| amber-go-s3 | `retention_cleanup` | `s3_connections` | 13 |
| amber-go-s3 | `retention_cleanup` | `s3_request_body_bytes` | 11.25 MiB |
| amber-go-s3 | `retention_cleanup` | `s3_requests_GET` | 2 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_POST` | 8 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_PUT` | 9 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_total` | 19 |
| amber-go-s3 | `retention_cleanup` | `s3_response_body_bytes` | 6.97 KiB |
| amber-go-s3 | `retention_cleanup` | `s3_responses_2xx` | 18 |
| amber-go-s3 | `retention_cleanup` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `retention_cleanup` | `stored_bytes` | 11.23 MiB |
| amber-go-s3 | `retention_cleanup` | `stored_objects` | 10 |

## Scenario `git/history`

### Elapsed time (median, lower is better)

| operation | git | amber-rust | amber-go |
|---|---|---|---|
| `history_store` | 183.0 ms | 117.9 ms | 312.3 ms |
| `clone_full` | 88.4 ms | unsupported | unsupported |
| `history_store_incremental` | 70.9 ms | 42.8 ms | 105.1 ms |
| `fetch_incremental` | 15.4 ms | unsupported | unsupported |
| `gc` | 94.7 ms | 5.4 ms | 8.7 ms |
| `list_head` | 1.3 ms | 111.4 ms | 140.6 ms |
| `read_all_versions` | 45.5 ms | 72.3 ms | 123.8 ms |
| `checkout_version` | 6.5 ms | 11.0 ms | 18.3 ms |
| `ref_delete` | 1.4 ms | 4.1 ms | 5.3 ms |

### Throughput over the logical bytes processed (median)

| operation | git | amber-rust | amber-go |
|---|---|---|---|
| `history_store` | 116.36 MiB/s | 179.36 MiB/s | 64.62 MiB/s |
| `clone_full` | — | unsupported | unsupported |
| `history_store_incremental` | 109.98 MiB/s | 178.15 MiB/s | 74.25 MiB/s |
| `fetch_incremental` | — | unsupported | unsupported |
| `gc` | — | — | — |
| `list_head` | — | — | — |
| `read_all_versions` | 636.84 MiB/s | 399.61 MiB/s | 230.14 MiB/s |
| `checkout_version` | — | — | — |
| `ref_delete` | — | — | — |

### CPU time, user + system (median)

| operation | git | amber-rust | amber-go |
|---|---|---|---|
| `history_store` | 179.8 ms | 437.4 ms | 2.46 s |
| `clone_full` | 117.5 ms | unsupported | unsupported |
| `history_store_incremental` | 69.6 ms | 151.3 ms | 845.6 ms |
| `fetch_incremental` | 16.7 ms | unsupported | unsupported |
| `gc` | 106.6 ms | 5.0 ms | 10.6 ms |
| `list_head` | 1.2 ms | 102.1 ms | 149.1 ms |
| `read_all_versions` | 44.1 ms | 67.5 ms | 186.9 ms |
| `checkout_version` | 6.4 ms | 10.6 ms | 27.0 ms |
| `ref_delete` | 1.3 ms | 3.8 ms | 5.6 ms |

### Peak resident set size (median)

| operation | git | amber-rust | amber-go |
|---|---|---|---|
| `history_store` | 38.58 MiB | 38.58 MiB | 163.76 MiB |
| `clone_full` | 38.58 MiB | unsupported | unsupported |
| `history_store_incremental` | 38.58 MiB | 38.58 MiB | 151.83 MiB |
| `fetch_incremental` | 38.58 MiB | unsupported | unsupported |
| `gc` | 38.58 MiB | 38.58 MiB | 38.66 MiB |
| `list_head` | 38.58 MiB | 38.58 MiB | 38.66 MiB |
| `read_all_versions` | 38.58 MiB | 38.58 MiB | 38.66 MiB |
| `checkout_version` | 38.58 MiB | 38.58 MiB | 38.66 MiB |
| `ref_delete` | 38.58 MiB | 38.58 MiB | 38.66 MiB |

### Storage allocated after each operation (median)

| operation | git | amber-rust | amber-go |
|---|---|---|---|
| `history_store` | 4.38 MiB | 3.86 MiB | 6.76 MiB |
| `clone_full` | — | unsupported | unsupported |
| `history_store_incremental` | 4.59 MiB | 3.88 MiB | 6.79 MiB |
| `fetch_incremental` | — | unsupported | unsupported |
| `gc` | 2.45 MiB | 3.89 MiB | 2.41 MiB |
| `list_head` | — | — | — |
| `read_all_versions` | — | — | — |
| `checkout_version` | — | — | — |
| `ref_delete` | — | — | — |

### Space reclaimed (median; positive means released)

| operation | git | amber-rust | amber-go |
|---|---|---|---|
| `history_store` | — | — | — |
| `clone_full` | — | unsupported | unsupported |
| `history_store_incremental` | — | — | — |
| `fetch_incremental` | — | unsupported | unsupported |
| `gc` | 2.14 MiB | -16.00 KiB | 4.38 MiB |
| `list_head` | — | — | — |
| `read_all_versions` | — | — | — |
| `checkout_version` | — | — | — |
| `ref_delete` | — | — | — |

### Dispersion of elapsed time

| backend | operation | n | median | p95 | min | max | stddev | cv |
|---|---|---|---|---|---|---|---|---|
| git | `history_store` | 2 | 183.0 ms | 184.6 ms | 183.0 ms | 184.6 ms | 1.1 ms | 0.6% |
| git | `clone_full` | 2 | 88.4 ms | 89.3 ms | 88.4 ms | 89.3 ms | 642.5 µs | 0.7% |
| git | `history_store_incremental` | 2 | 70.9 ms | 71.0 ms | 70.9 ms | 71.0 ms | 108.2 µs | 0.2% |
| git | `fetch_incremental` | 2 | 15.4 ms | 15.6 ms | 15.4 ms | 15.6 ms | 101.7 µs | 0.7% |
| git | `gc` | 2 | 94.7 ms | 99.1 ms | 94.7 ms | 99.1 ms | 3.2 ms | 3.3% |
| git | `list_head` | 2 | 1.3 ms | 1.4 ms | 1.3 ms | 1.4 ms | 69.1 µs | 5.1% |
| git | `read_all_versions` | 2 | 45.5 ms | 46.0 ms | 45.5 ms | 46.0 ms | 328.9 µs | 0.7% |
| git | `checkout_version` | 2 | 6.5 ms | 6.9 ms | 6.5 ms | 6.9 ms | 227.6 µs | 3.4% |
| git | `ref_delete` | 2 | 1.4 ms | 1.4 ms | 1.4 ms | 1.4 ms | 60.4 µs | 4.3% |
| amber-rust | `history_store` | 2 | 117.9 ms | 119.8 ms | 117.9 ms | 119.8 ms | 1.3 ms | 1.1% |
| amber-rust | `history_store_incremental` | 2 | 42.8 ms | 43.9 ms | 42.8 ms | 43.9 ms | 776.1 µs | 1.8% |
| amber-rust | `gc` | 2 | 5.4 ms | 5.5 ms | 5.4 ms | 5.5 ms | 30.0 µs | 0.5% |
| amber-rust | `list_head` | 2 | 111.4 ms | 120.3 ms | 111.4 ms | 120.3 ms | 6.3 ms | 5.4% |
| amber-rust | `read_all_versions` | 2 | 72.3 ms | 73.3 ms | 72.3 ms | 73.3 ms | 704.4 µs | 1.0% |
| amber-rust | `checkout_version` | 2 | 11.0 ms | 11.1 ms | 11.0 ms | 11.1 ms | 103.7 µs | 0.9% |
| amber-rust | `ref_delete` | 2 | 4.1 ms | 4.6 ms | 4.1 ms | 4.6 ms | 342.2 µs | 7.8% |
| amber-go | `history_store` | 2 | 312.3 ms | 332.4 ms | 312.3 ms | 332.4 ms | 14.2 ms | 4.4% |
| amber-go | `history_store_incremental` | 2 | 105.1 ms | 105.2 ms | 105.1 ms | 105.2 ms | 88.8 µs | 0.1% |
| amber-go | `gc` | 2 | 8.7 ms | 8.7 ms | 8.7 ms | 8.7 ms | 47.2 µs | 0.5% |
| amber-go | `list_head` | 2 | 140.6 ms | 141.7 ms | 140.6 ms | 141.7 ms | 786.6 µs | 0.6% |
| amber-go | `read_all_versions` | 2 | 123.8 ms | 127.3 ms | 123.8 ms | 127.3 ms | 2.5 ms | 2.0% |
| amber-go | `checkout_version` | 2 | 18.3 ms | 18.4 ms | 18.3 ms | 18.4 ms | 74.6 µs | 0.4% |
| amber-go | `ref_delete` | 2 | 5.3 ms | 5.5 ms | 5.3 ms | 5.5 ms | 183.1 µs | 3.4% |

### What these numbers mean, operation by operation

Read these before comparing two cells above: they state what each measured operation actually delivered.

| backend | operation | note |
|---|---|---|
| amber-go | `history_store` | 11 commands in total; the first few are listed |
| amber-go | `history_store_incremental` | 4 commands in total; the first few are listed |
| amber-go | `list_head` | 29 separate `amber-store ls` processes, one per directory: neither core's CLI has a recursive listing |
| amber-rust | `history_store` | 11 commands in total; the first few are listed |
| amber-rust | `history_store_incremental` | 4 commands in total; the first few are listed |
| amber-rust | `list_head` | 29 separate `amber-store ls` processes, one per directory: neither core's CLI has a recursive listing |
| git | `history_store` | 27 commands in total; the first few are listed |
| git | `history_store_incremental` | 10 commands in total; the first few are listed |

### Recorded counters (median per operation)

| backend | operation | counter | median |
|---|---|---|---|
| git | `history_store` | `backend_commands` | 27 |
| git | `history_store` | `commits` | 11 |
| git | `history_store` | `store_refs_allocated_bytes` | 36.00 KiB |
| git | `history_store` | `store_refs_apparent_bytes` | 205 B |
| git | `history_store_incremental` | `backend_commands` | 10 |
| git | `history_store_incremental` | `commits` | 4 |
| git | `history_store_incremental` | `store_refs_allocated_bytes` | 36.00 KiB |
| git | `history_store_incremental` | `store_refs_apparent_bytes` | 205 B |
| amber-rust | `history_store` | `backend_commands` | 11 |
| amber-rust | `history_store` | `commits` | 11 |
| amber-rust | `history_store` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `history_store` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `history_store` | `store_packstore_allocated_bytes` | 2.29 MiB |
| amber-rust | `history_store` | `store_packstore_apparent_bytes` | 2.29 MiB |
| amber-rust | `history_store` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `history_store` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `history_store_incremental` | `backend_commands` | 4 |
| amber-rust | `history_store_incremental` | `commits` | 4 |
| amber-rust | `history_store_incremental` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `history_store_incremental` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `history_store_incremental` | `store_packstore_allocated_bytes` | 2.31 MiB |
| amber-rust | `history_store_incremental` | `store_packstore_apparent_bytes` | 2.30 MiB |
| amber-rust | `history_store_incremental` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `history_store_incremental` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `list_head` | `cli_invocations` | 29 |
| amber-rust | `list_head` | `entries` | 169 |
| amber-go | `history_store` | `backend_commands` | 11 |
| amber-go | `history_store` | `commits` | 11 |
| amber-go | `history_store` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `history_store` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `history_store` | `store_packstore_allocated_bytes` | 2.29 MiB |
| amber-go | `history_store` | `store_packstore_apparent_bytes` | 2.29 MiB |
| amber-go | `history_store` | `store_refs_allocated_bytes` | 4.46 MiB |
| amber-go | `history_store` | `store_refs_apparent_bytes` | 10.18 KiB |
| amber-go | `history_store_incremental` | `backend_commands` | 4 |
| amber-go | `history_store_incremental` | `commits` | 4 |
| amber-go | `history_store_incremental` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `history_store_incremental` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `history_store_incremental` | `store_packstore_allocated_bytes` | 2.31 MiB |
| amber-go | `history_store_incremental` | `store_packstore_apparent_bytes` | 2.30 MiB |
| amber-go | `history_store_incremental` | `store_refs_allocated_bytes` | 4.47 MiB |
| amber-go | `history_store_incremental` | `store_refs_apparent_bytes` | 13.10 KiB |
| amber-go | `list_head` | `cli_invocations` | 29 |
| amber-go | `list_head` | `entries` | 169 |

## What each backend guarantees

These differences are the reason a single ranking would be meaningless. Read them before comparing any two rows.

### `amber-go-s3` in `blob/nix-closure`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* **Transport:** harness transport, not a product protocol: neither core ships any network client, so the harness publishes the store's on-disk pack segments and reference database with `mc mirror`. Sealed segments are immutable and named by id, so an incremental publication uploads only new ones; the active segment and the reference database are re-uploaded whenever they change. This is a file-level mirror, directly comparable with Git's dumb publication and not with restic's or Nix's native S3 clients.
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.
* object store: local garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] started by the harness; gateway shaping: none: the loopback path to the local object store was measured as it is
* the two pulls state what they delivered: pull_fresh brings a client that has never seen the store up to everything published at that point, and pull_incremental brings the same client up to date after the incremental push. Both record delivered_references and delivered_logical_bytes, and every delivered reference is verified at the destination.
* these cores cannot fetch a single reference: a client that wants one published tree has to download the whole store. The delivered set recorded on each pull therefore covers every reference published so far, and every one of them is restored and verified — which is the honest comparison against a backend that can fetch one snapshot.
* delivered set: 8 reference(s) in the initial publication and 9 in the incremental one, 17 in total

### `amber-go` in `nix/closure`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.
* closure source: the deterministic fixture closure built by this flake. Generation 1 has 8 paths and generation 2 has 9, sharing 6 of them
* the fixture contains executables, relative and absolute symlinks, a dangling symlink and read-only directories

### `amber-go-s3` in `blob/source-history`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* **Transport:** harness transport, not a product protocol: neither core ships any network client, so the harness publishes the store's on-disk pack segments and reference database with `mc mirror`. Sealed segments are immutable and named by id, so an incremental publication uploads only new ones; the active segment and the reference database are re-uploaded whenever they change. This is a file-level mirror, directly comparable with Git's dumb publication and not with restic's or Nix's native S3 clients.
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.
* object store: local garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] started by the harness; gateway shaping: none: the loopback path to the local object store was measured as it is
* the two pulls state what they delivered: pull_fresh brings a client that has never seen the store up to everything published at that point, and pull_incremental brings the same client up to date after the incremental push. Both record delivered_references and delivered_logical_bytes, and every delivered reference is verified at the destination.
* these cores cannot fetch a single reference: a client that wants one published tree has to download the whole store. The delivered set recorded on each pull therefore covers every reference published so far, and every one of them is restored and verified — which is the honest comparison against a backend that can fetch one snapshot.
* delivered set: 11 reference(s) in the initial publication and 4 in the incremental one, 15 in total

### `restic` in `tree/lifecycle`

* **Compression:** zstd (repository format 2), enabled by default
* **Encryption:** mandatory AES-256-CTR with Poly1305-AES authentication; every byte stored is encrypted and authenticated. This is real work the other backends do not do, and it is the main reason restic's CPU column is not comparable to theirs.
* **Durability:** pack files and index files are written then fsynced; snapshots are committed last
* **Concurrency:** parallel file readers and uploaders
* restic deduplicates with content-defined chunking, like the Amber cores, but with its own parameters (512 KiB average). The harness does not try to equalise them: they are not exposed on restic's command line.

### `amber-rust-s3` in `blob/source-history`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* **Transport:** harness transport, not a product protocol: neither core ships any network client, so the harness publishes the store's on-disk pack segments and reference database with `mc mirror`. Sealed segments are immutable and named by id, so an incremental publication uploads only new ones; the active segment and the reference database are re-uploaded whenever they change. This is a file-level mirror, directly comparable with Git's dumb publication and not with restic's or Nix's native S3 clients.
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.
* object store: local garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] started by the harness; gateway shaping: none: the loopback path to the local object store was measured as it is
* the two pulls state what they delivered: pull_fresh brings a client that has never seen the store up to everything published at that point, and pull_incremental brings the same client up to date after the incremental push. Both record delivered_references and delivered_logical_bytes, and every delivered reference is verified at the destination.
* these cores cannot fetch a single reference: a client that wants one published tree has to download the whole store. The delivered set recorded on each pull therefore covers every reference published so far, and every one of them is restored and verified — which is the honest comparison against a backend that can fetch one snapshot.
* delivered set: 11 reference(s) in the initial publication and 4 in the incremental one, 15 in total

### `amber-rust` in `backup/retention`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.

### `restic-s3` in `blob/backup-corpus`

* **Compression:** zstd (repository format 2), enabled by default
* **Encryption:** mandatory AES-256-CTR with Poly1305-AES authentication; every byte stored is encrypted and authenticated. This is real work the other backends do not do, and it is the main reason restic's CPU column is not comparable to theirs.
* **Durability:** pack files and index files are written then fsynced; snapshots are committed last
* **Concurrency:** parallel file readers and uploaders
* **Transport:** restic's own S3 backend (minio-go), speaking S3 directly to the object store. This is a native, supported transport.
* restic deduplicates with content-defined chunking, like the Amber cores, but with its own parameters (512 KiB average). The harness does not try to equalise them: they are not exposed on restic's command line.
* object store: local garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] started by the harness; gateway shaping: none: the loopback path to the local object store was measured as it is
* the two pulls state what they delivered: pull_fresh brings a client that has never seen the store up to everything published at that point, and pull_incremental brings the same client up to date after the incremental push. Both record delivered_references and delivered_logical_bytes, and every delivered reference is verified at the destination.

### `nix-binary-cache` in `blob/nix-closure`

* **Compression:** none in a local store; xz for NARs published to a binary cache
* **Encryption:** none; integrity comes from NAR hashes and optional signatures
* **Durability:** store paths are built or unpacked into a temporary name, made read-only, then registered in the SQLite database
* **Concurrency:** parallel substitution and copying
* **Transport:** Nix's own S3 binary-cache store (`nix copy --to s3://...`), speaking S3 directly to the object store. This is a native, supported transport.
* A Nix store is not a general filesystem-tree store: it holds whole store paths with recorded references, and deduplicates only by whole path (plus optional hard-linking of identical files). Comparisons here are storage-layer comparisons.
* object store: local garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] started by the harness; gateway shaping: none: the loopback path to the local object store was measured as it is
* the two pulls state what they delivered: pull_fresh brings a client that has never seen the store up to everything published at that point, and pull_incremental brings the same client up to date after the incremental push. Both record delivered_references and delivered_logical_bytes, and every delivered reference is verified at the destination.
* the binary cache is published under the key prefix s3://amber-cas-bench/bench/smoke-1789241936307339195/nix-closure/nix-binary-cache/rep0/cache — Nix carries a cache's key prefix in the path after the bucket, and the harness checks the keys that actually appeared rather than assuming the URI was honoured

### `fs-sha256` in `tree/lifecycle`

* **Compression:** none, by construction
* **Encryption:** none
* **Durability:** objects are written to a temporary name, fsynced and renamed; the reference file is fsynced last
* **Concurrency:** single-threaded, by construction
* This is the control, not a product: whole-file SHA-256 addressing with no chunking and no compression. It shows what the workload costs with every clever technique removed.

### `amber-rust-s3` in `blob/backup-corpus`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* **Transport:** harness transport, not a product protocol: neither core ships any network client, so the harness publishes the store's on-disk pack segments and reference database with `mc mirror`. Sealed segments are immutable and named by id, so an incremental publication uploads only new ones; the active segment and the reference database are re-uploaded whenever they change. This is a file-level mirror, directly comparable with Git's dumb publication and not with restic's or Nix's native S3 clients.
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.
* object store: local garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] started by the harness; gateway shaping: none: the loopback path to the local object store was measured as it is
* the two pulls state what they delivered: pull_fresh brings a client that has never seen the store up to everything published at that point, and pull_incremental brings the same client up to date after the incremental push. Both record delivered_references and delivered_logical_bytes, and every delivered reference is verified at the destination.
* these cores cannot fetch a single reference: a client that wants one published tree has to download the whole store. The delivered set recorded on each pull therefore covers every reference published so far, and every one of them is restored and verified — which is the honest comparison against a backend that can fetch one snapshot.
* delivered set: 1 reference(s) in the initial publication and 1 in the incremental one, 2 in total

### `git` in `git/history`

* **Compression:** zlib per object, plus delta compression inside packs
* **Encryption:** none
* **Durability:** loose objects and packs are written then renamed; `core.fsyncObjectFiles` is left at its default
* **Concurrency:** single-threaded for commits; packing uses --threads (left at its default)
* Git stores whole file versions, deltified against other versions at pack time. It records no modification times, so restored trees are compared without them.
* retained versions are not compared on permission bits other than the executable bit: this backend does not store it
* retained versions are not compared on modification times: this backend does not store it
* retained versions are not compared on empty directories: this backend does not store it

### `amber-rust` in `git/history`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.

### `amber-rust-s3` in `blob/nix-closure`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* **Transport:** harness transport, not a product protocol: neither core ships any network client, so the harness publishes the store's on-disk pack segments and reference database with `mc mirror`. Sealed segments are immutable and named by id, so an incremental publication uploads only new ones; the active segment and the reference database are re-uploaded whenever they change. This is a file-level mirror, directly comparable with Git's dumb publication and not with restic's or Nix's native S3 clients.
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.
* object store: local garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] started by the harness; gateway shaping: none: the loopback path to the local object store was measured as it is
* the two pulls state what they delivered: pull_fresh brings a client that has never seen the store up to everything published at that point, and pull_incremental brings the same client up to date after the incremental push. Both record delivered_references and delivered_logical_bytes, and every delivered reference is verified at the destination.
* these cores cannot fetch a single reference: a client that wants one published tree has to download the whole store. The delivered set recorded on each pull therefore covers every reference published so far, and every one of them is restored and verified — which is the honest comparison against a backend that can fetch one snapshot.
* delivered set: 8 reference(s) in the initial publication and 9 in the incremental one, 17 in total

### `amber-go-s3` in `blob/backup-corpus`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* **Transport:** harness transport, not a product protocol: neither core ships any network client, so the harness publishes the store's on-disk pack segments and reference database with `mc mirror`. Sealed segments are immutable and named by id, so an incremental publication uploads only new ones; the active segment and the reference database are re-uploaded whenever they change. This is a file-level mirror, directly comparable with Git's dumb publication and not with restic's or Nix's native S3 clients.
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.
* object store: local garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] started by the harness; gateway shaping: none: the loopback path to the local object store was measured as it is
* the two pulls state what they delivered: pull_fresh brings a client that has never seen the store up to everything published at that point, and pull_incremental brings the same client up to date after the incremental push. Both record delivered_references and delivered_logical_bytes, and every delivered reference is verified at the destination.
* these cores cannot fetch a single reference: a client that wants one published tree has to download the whole store. The delivered set recorded on each pull therefore covers every reference published so far, and every one of them is restored and verified — which is the honest comparison against a backend that can fetch one snapshot.
* delivered set: 1 reference(s) in the initial publication and 1 in the incremental one, 2 in total

### `git-bundle-s3` in `blob/source-history`

* **Compression:** zlib per object, plus delta compression inside packs
* **Encryption:** none
* **Durability:** loose objects and packs are written then renamed; `core.fsyncObjectFiles` is left at its default
* **Concurrency:** single-threaded for commits; packing uses --threads (left at its default)
* **Transport:** Git bundle publication: `git bundle create` writes the repository (or an increment of it) to a file, which is then uploaded with the MinIO client. Git has no native protocol for pushing to arbitrary S3 object storage, so the object transfer is the harness's, and the numbers below include the cost of producing the bundle as well as of uploading it.
* Git stores whole file versions, deltified against other versions at pack time. It records no modification times, so restored trees are compared without them.
* object store: local garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] started by the harness; gateway shaping: none: the loopback path to the local object store was measured as it is
* the two pulls state what they delivered: pull_fresh brings a client that has never seen the store up to everything published at that point, and pull_incremental brings the same client up to date after the incremental push. Both record delivered_references and delivered_logical_bytes, and every delivered reference is verified at the destination.

### `amber-go` in `backup/retention`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.

### `nix` in `nix/closure`

* **Compression:** none in a local store; xz for NARs published to a binary cache
* **Encryption:** none; integrity comes from NAR hashes and optional signatures
* **Durability:** store paths are built or unpacked into a temporary name, made read-only, then registered in the SQLite database
* **Concurrency:** parallel substitution and copying
* A Nix store is not a general filesystem-tree store: it holds whole store paths with recorded references, and deduplicates only by whole path (plus optional hard-linking of identical files). Comparisons here are storage-layer comparisons.
* closure source: the deterministic fixture closure built by this flake. Generation 1 has 8 paths and generation 2 has 9, sharing 6 of them
* the fixture contains executables, relative and absolute symlinks, a dangling symlink and read-only directories

### `amber-go` in `tree/lifecycle`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.

### `amber-rust` in `nix/closure`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.
* closure source: the deterministic fixture closure built by this flake. Generation 1 has 8 paths and generation 2 has 9, sharing 6 of them
* the fixture contains executables, relative and absolute symlinks, a dangling symlink and read-only directories

### `amber-go` in `git/history`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.

### `restic` in `backup/retention`

* **Compression:** zstd (repository format 2), enabled by default
* **Encryption:** mandatory AES-256-CTR with Poly1305-AES authentication; every byte stored is encrypted and authenticated. This is real work the other backends do not do, and it is the main reason restic's CPU column is not comparable to theirs.
* **Durability:** pack files and index files are written then fsynced; snapshots are committed last
* **Concurrency:** parallel file readers and uploaders
* restic deduplicates with content-defined chunking, like the Amber cores, but with its own parameters (512 KiB average). The harness does not try to equalise them: they are not exposed on restic's command line.

### `git` in `tree/lifecycle`

* **Compression:** zlib per object, plus delta compression inside packs
* **Encryption:** none
* **Durability:** loose objects and packs are written then renamed; `core.fsyncObjectFiles` is left at its default
* **Concurrency:** single-threaded for commits; packing uses --threads (left at its default)
* Git stores whole file versions, deltified against other versions at pack time. It records no modification times, so restored trees are compared without them.
* restored trees are not compared on permission bits other than the executable bit: this backend does not store it
* restored trees are not compared on modification times: this backend does not store it
* restored trees are not compared on empty directories: this backend does not store it

### `amber-rust` in `tree/lifecycle`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.

## Unsupported operations

These are not zeroes and not omissions: the backend has no equivalent of the operation.

| scenario | backend | operation | why |
|---|---|---|---|
| `nix/closure` | amber-go | `closure_query` | these cores store trees keyed by content and record no references between roots, so there is no closure to query. The harness keeps the reference graph itself and verifies completeness separately; see the closure_is_complete check. |
| `nix/closure` | amber-go | `nar_and_reference_registration` | these cores record no NAR hash and no reference list for a stored tree: they address content by their own key and model no edges between roots. The harness therefore recomputes the NAR hash of every restored path with `nix hash path` and compares it with the source store's — see the nar_hashes_match_recomputed check — but nothing in the store under test would have noticed a mismatch. |
| `nix/closure` | amber-go | `cache_export` | these cores have no binary-cache publication format; the blob scenario group measures what they can do, which is to publish their on-disk pack segments to object storage |
| `nix/closure` | amber-go | `cache_substitute` | these cores have no binary-cache client; see the blob scenario group |
| `git/history` | amber-rust | `clone_full` | these cores ship no repository-to-repository transfer command; what they can do to publish a store is measured in the blob scenario group as a file-level object-storage transport |
| `git/history` | amber-rust | `fetch_incremental` | these cores ship no incremental fetch command; see the blob scenario group for incremental object-storage publication |
| `nix/closure` | amber-rust | `closure_query` | these cores store trees keyed by content and record no references between roots, so there is no closure to query. The harness keeps the reference graph itself and verifies completeness separately; see the closure_is_complete check. |
| `nix/closure` | amber-rust | `nar_and_reference_registration` | these cores record no NAR hash and no reference list for a stored tree: they address content by their own key and model no edges between roots. The harness therefore recomputes the NAR hash of every restored path with `nix hash path` and compares it with the source store's — see the nar_hashes_match_recomputed check — but nothing in the store under test would have noticed a mismatch. |
| `nix/closure` | amber-rust | `cache_export` | these cores have no binary-cache publication format; the blob scenario group measures what they can do, which is to publish their on-disk pack segments to object storage |
| `nix/closure` | amber-rust | `cache_substitute` | these cores have no binary-cache client; see the blob scenario group |
| `git/history` | amber-go | `clone_full` | these cores ship no repository-to-repository transfer command; what they can do to publish a store is measured in the blob scenario group as a file-level object-storage transport |
| `git/history` | amber-go | `fetch_incremental` | these cores ship no incremental fetch command; see the blob scenario group for incremental object-storage publication |

## Correctness checks

| scenario | backend | check | result | detail |
|---|---|---|---|---|
| `backup/retention` | amber-go | `every_retained_snapshot_matches_manifest` | pass (2/2) | all 2 retained snapshots (snap1, snap2) were restored after retention and matched their manifests byte for byte |
| `backup/retention` | amber-go | `forgotten_snapshots_are_absent` | pass (2/2) | the store lists exactly what retention should have left: retained 2 (snap1, snap2), dropped 2 (snap0, snap0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `backup/retention` | amber-go | `restore_matches_manifest` | pass (2/2) | 284 entries expected under backup-amber-go-rep0/restore-latest; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `backup/retention` | amber-rust | `every_retained_snapshot_matches_manifest` | pass (2/2) | all 2 retained snapshots (snap1, snap2) were restored after retention and matched their manifests byte for byte |
| `backup/retention` | amber-rust | `forgotten_snapshots_are_absent` | pass (2/2) | the store lists exactly what retention should have left: retained 2 (snap1, snap2), dropped 2 (snap0, snap0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `backup/retention` | amber-rust | `restore_matches_manifest` | pass (2/2) | 284 entries expected under backup-amber-rust-rep0/restore-latest; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `backup/retention` | restic | `every_retained_snapshot_matches_manifest` | pass (2/2) | all 2 retained snapshots (snap1, snap2) were restored after retention and matched their manifests byte for byte |
| `backup/retention` | restic | `forgotten_snapshots_are_absent` | pass (2/2) | the store lists exactly what retention should have left: retained 2 (snap1, snap2), dropped 2 (snap0, snap0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `backup/retention` | restic | `restore_matches_manifest` | pass (2/2) | 284 entries expected under backup-restic-rep0/restore-latest; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `blob/backup-corpus` | amber-go-s3 | `dropped_references_are_absent` | pass (2/2) | the store lists exactly what retention should have left: retained 1 (gen1), dropped 1 (gen0); absence was read from the backend's own successful listing, not inferred from a failed command |
| `blob/backup-corpus` | amber-go-s3 | `fresh_pull_restores_correctly` | pass (2/2) | 1 of the 1 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/backup-corpus` | amber-go-s3 | `incremental_pull_restores_correctly` | pass (2/2) | 2 of the 2 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/backup-corpus` | amber-go-s3 | `missing_object_is_detected` | pass (2/2) | after deleting backup-corpus/amber-go-s3/rep0/store/packstore/0000000000000003.seg.active, the backend failed rather than returning an incomplete result: /nix/store/y73dri4kbmb0jx08kdi93cyyxphjk2d6-amber-store-go-0.0.7-4ed4660657b1/bin/amber-store --store <scratch>/runs/blob-backup-corpus-amber-go-s3-rep0/pulled-probe --segment-size 8388608 export ref:gen1: exited 1 |
| `blob/backup-corpus` | amber-go-s3 | `retained_content_valid_after_cleanup` | pass (2/2) | 1 of the 1 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/backup-corpus` | amber-rust-s3 | `dropped_references_are_absent` | pass (2/2) | the store lists exactly what retention should have left: retained 1 (gen1), dropped 1 (gen0); absence was read from the backend's own successful listing, not inferred from a failed command |
| `blob/backup-corpus` | amber-rust-s3 | `fresh_pull_restores_correctly` | pass (2/2) | 1 of the 1 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/backup-corpus` | amber-rust-s3 | `incremental_pull_restores_correctly` | pass (2/2) | 2 of the 2 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/backup-corpus` | amber-rust-s3 | `missing_object_is_detected` | pass (2/2) | after deleting backup-corpus/amber-rust-s3/rep0/store/packstore/0000000000000003.seg.active, the backend failed rather than returning an incomplete result: /nix/store/5cqjvnw3zg461xww08q9gj3lwd8v2iz1-amber-store-rust-0.2.0-141df2b0a9a8/bin/amber-store --store <scratch>/runs/blob-backup-corpus-amber-rust-s3-rep0/pulled-probe --segment-size 8388608 export ref:gen1: exited 1 |
| `blob/backup-corpus` | amber-rust-s3 | `retained_content_valid_after_cleanup` | pass (2/2) | 1 of the 1 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/backup-corpus` | restic-s3 | `dropped_snapshots_are_absent` | pass (2/2) | the store lists exactly what retention should have left: retained 1 (g1), dropped 2 (g0, g0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `blob/backup-corpus` | restic-s3 | `earlier_delivery_still_matches_manifest` | pass (2/2) | 283 entries expected under blob-backup-corpus-restic-s3-rep0/pull-initial; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `blob/backup-corpus` | restic-s3 | `fresh_pull_matches_manifest` | pass (2/2) | 283 entries expected under blob-backup-corpus-restic-s3-rep0/pull-initial; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `blob/backup-corpus` | restic-s3 | `incremental_pull_matches_manifest` | pass (2/2) | 284 entries expected under blob-backup-corpus-restic-s3-rep0/pull-incremental; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `blob/backup-corpus` | restic-s3 | `missing_object_is_detected` | pass (2/2) | after deleting backup-corpus/restic-s3/rep0/repo/data/83/83930eb389c96cc98f2f4ad002e7a78d7f45bfd852bc4fd020ce0431e35b0900, the backend failed rather than returning an incomplete result: /nix/store/jh7vrxar93sk4m8igr35zdlgrz3nakrk-restic-0.18.1/bin/restic -r s3:http://127.0.0.1:39091/amber-cas-bench/bench/smoke-1789241936307339195/backup-corpus/restic-s3/rep0/repo --password-file <scratch>/runs/blob-backup-corpus-restic-s3-rep0/restic-password --cache-dir <scratch>/runs/blob-backup-corpus-restic-s3-rep0/cache-post check --read-data: exited 1 |
| `blob/backup-corpus` | restic-s3 | `retained_content_valid_after_cleanup` | pass (2/2) | 284 entries expected under blob-backup-corpus-restic-s3-rep0/pull-after-cleanup; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `blob/nix-closure` | amber-go-s3 | `dropped_references_are_absent` | pass (2/2) | the store lists exactly what retention should have left: retained 9 (nixpath-85isfxk8vh94rs8sqabjpbcj9sdxn3vw-amber-bench-mid-a-smoke, nixpath-b1s531nnp12fsa45ybdas49dy42a28v7-amber-bench-data-text-g2-smoke, nixpath-cqa27c2j3j8lah2y78wmmnn155pzh1gj-amber-bench-data-blob-g1-smoke, nixpath-hg39mscg3a5ii2idh96gnm3fmzyiz9cj-amber-bench-lib-late-smoke, nixpath-kqlq8y2qpin7da6xhkz2fzwqlq7lnz5n-amber-bench-lib-extra-smoke, nixpath-p62b4x8ynxq6n8lzzkpcfay0qwp2spjz-amber-bench-lib-shared-smoke, nixpath-q0b26i8gjpj5zyhqx8mngahg31dl9qsl-amber-bench-lib-core-smoke, nixpath-va92drjki75lypdl3lgbl1kzhxakdiys-amber-bench-mid-b-smoke, nixpath-ysxxyx9gy9y34lnwsxr04z4dwhwv43wn-amber-bench-app-gen2-smoke), dropped 2 (nixpath-jzs0r13qazz57ymhcbd4k1yad4156g2d-amber-bench-data-text-g1-smoke, nixpath-x2g1v81yvf9gikpv2b2jbilpc02af154-amber-bench-app-gen1-smoke); absence was read from the backend's own successful listing, not inferred from a failed command |
| `blob/nix-closure` | amber-go-s3 | `fresh_pull_restores_correctly` | pass (2/2) | 8 of the 8 delivered reference(s) restored from the downloaded store and matched byte for byte; 8 of them also reproduced the NAR hash the source Nix store recorded, which is every one of them; 0 unverified |
| `blob/nix-closure` | amber-go-s3 | `incremental_pull_restores_correctly` | pass (2/2) | 17 of the 17 delivered reference(s) restored from the downloaded store and matched byte for byte; 17 of them also reproduced the NAR hash the source Nix store recorded, which is every one of them; 0 unverified |
| `blob/nix-closure` | amber-go-s3 | `missing_object_is_detected` | pass (2/2) | after deleting nix-closure/amber-go-s3/rep0/store/packstore/0000000000000001.seg, the backend failed rather than returning an incomplete result: /nix/store/y73dri4kbmb0jx08kdi93cyyxphjk2d6-amber-store-go-0.0.7-4ed4660657b1/bin/amber-store --store <scratch>/runs/blob-nix-closure-amber-go-s3-rep0/pulled-probe --segment-size 8388608 export ref:nixpath-85isfxk8vh94rs8sqabjpbcj9sdxn3vw-amber-bench-mid-a-smoke: exited 1 |
| `blob/nix-closure` | amber-go-s3 | `retained_content_valid_after_cleanup` | pass (2/2) | 9 of the 9 delivered reference(s) restored from the downloaded store and matched byte for byte; 9 of them also reproduced the NAR hash the source Nix store recorded, which is every one of them; 0 unverified |
| `blob/nix-closure` | amber-rust-s3 | `dropped_references_are_absent` | pass (2/2) | the store lists exactly what retention should have left: retained 9 (nixpath-85isfxk8vh94rs8sqabjpbcj9sdxn3vw-amber-bench-mid-a-smoke, nixpath-b1s531nnp12fsa45ybdas49dy42a28v7-amber-bench-data-text-g2-smoke, nixpath-cqa27c2j3j8lah2y78wmmnn155pzh1gj-amber-bench-data-blob-g1-smoke, nixpath-hg39mscg3a5ii2idh96gnm3fmzyiz9cj-amber-bench-lib-late-smoke, nixpath-kqlq8y2qpin7da6xhkz2fzwqlq7lnz5n-amber-bench-lib-extra-smoke, nixpath-p62b4x8ynxq6n8lzzkpcfay0qwp2spjz-amber-bench-lib-shared-smoke, nixpath-q0b26i8gjpj5zyhqx8mngahg31dl9qsl-amber-bench-lib-core-smoke, nixpath-va92drjki75lypdl3lgbl1kzhxakdiys-amber-bench-mid-b-smoke, nixpath-ysxxyx9gy9y34lnwsxr04z4dwhwv43wn-amber-bench-app-gen2-smoke), dropped 2 (nixpath-jzs0r13qazz57ymhcbd4k1yad4156g2d-amber-bench-data-text-g1-smoke, nixpath-x2g1v81yvf9gikpv2b2jbilpc02af154-amber-bench-app-gen1-smoke); absence was read from the backend's own successful listing, not inferred from a failed command |
| `blob/nix-closure` | amber-rust-s3 | `fresh_pull_restores_correctly` | pass (2/2) | 8 of the 8 delivered reference(s) restored from the downloaded store and matched byte for byte; 8 of them also reproduced the NAR hash the source Nix store recorded, which is every one of them; 0 unverified |
| `blob/nix-closure` | amber-rust-s3 | `incremental_pull_restores_correctly` | pass (2/2) | 17 of the 17 delivered reference(s) restored from the downloaded store and matched byte for byte; 17 of them also reproduced the NAR hash the source Nix store recorded, which is every one of them; 0 unverified |
| `blob/nix-closure` | amber-rust-s3 | `missing_object_is_detected` | pass (2/2) | after deleting nix-closure/amber-rust-s3/rep0/store/packstore/0000000000000001.seg, the backend failed rather than returning an incomplete result: /nix/store/5cqjvnw3zg461xww08q9gj3lwd8v2iz1-amber-store-rust-0.2.0-141df2b0a9a8/bin/amber-store --store <scratch>/runs/blob-nix-closure-amber-rust-s3-rep0/pulled-probe --segment-size 8388608 export ref:nixpath-85isfxk8vh94rs8sqabjpbcj9sdxn3vw-amber-bench-mid-a-smoke: exited 1 |
| `blob/nix-closure` | amber-rust-s3 | `retained_content_valid_after_cleanup` | pass (2/2) | 9 of the 9 delivered reference(s) restored from the downloaded store and matched byte for byte; 9 of them also reproduced the NAR hash the source Nix store recorded, which is every one of them; 0 unverified |
| `blob/nix-closure` | nix-binary-cache | `earlier_delivery_still_matches_source` | pass (2/2) | all 8 paths of the closure arrived: the destination registers each one with the source store's NAR hash and reference list, 8 of them reproduce that hash when recomputed from the bytes on disk with `nix hash path`, the destination holds nothing extra, and every reference of every path is inside the delivered set |
| `blob/nix-closure` | nix-binary-cache | `expired_narinfos_are_absent` | pass (2/2) | all 2 expired narinfo object(s) are absent from a successful listing of the 21 object(s) the cache still holds, and a client with its own empty narinfo cache obtains none of the 2 store paths only the superseded generation needed |
| `blob/nix-closure` | nix-binary-cache | `fresh_pull_matches_source` | pass (2/2) | all 8 paths of the closure arrived: the destination registers each one with the source store's NAR hash and reference list, 8 of them reproduce that hash when recomputed from the bytes on disk with `nix hash path`, the destination holds nothing extra, and every reference of every path is inside the delivered set |
| `blob/nix-closure` | nix-binary-cache | `incremental_pull_matches_source` | pass (2/2) | all 9 paths of the closure arrived: the destination registers each one with the source store's NAR hash and reference list, 9 of them reproduce that hash when recomputed from the bytes on disk with `nix hash path`, the destination holds nothing extra, and every reference of every path is inside the delivered set |
| `blob/nix-closure` | nix-binary-cache | `missing_object_is_detected` | pass (2/2) | after deleting nix-closure/nix-binary-cache/rep0/cache/nar/19wzk9i2ll9v9ywarpvvwwgv3v3db7c6xbv158lk4xwpxfxifaz1.nar.xz, the backend failed rather than returning an incomplete result: /nix/store/0djmqy9c50czxaakvi7xqinz30wlbs6g-nix-2.34.8/bin/nix --extra-experimental-features "nix-command flakes" --option substituters  --option extra-substituters  --option trusted-substituters  --option builders  copy --no-check-sigs --from s3://amber-cas-bench/bench/smoke-1789241936307339195/nix-closure/nix-binary-cache/rep0/cache?endpoint=127.0.0.1:39091&region=garage&scheme=http --to <scratch>/runs/blob-nix-closure-nix-binary-cache-rep0/store-probe /nix/store/ysxxyx9gy9y34lnwsxr04z4dwhwv43wn-amber-bench-app-gen2-smoke: exited 1 |
| `blob/nix-closure` | nix-binary-cache | `published_keys_stay_inside_the_namespace` | pass (2/2) | 17 object(s) appeared and every one of them is under amber-cas-bench/bench/smoke-1789241936307339195/nix-closure/nix-binary-cache/rep0/cache, which is what the store URI asked for |
| `blob/nix-closure` | nix-binary-cache | `retained_content_valid_after_cleanup` | pass (2/2) | all 9 paths of the closure arrived: the destination registers each one with the source store's NAR hash and reference list, 9 of them reproduce that hash when recomputed from the bytes on disk with `nix hash path`, the destination holds nothing extra, and every reference of every path is inside the delivered set |
| `blob/source-history` | amber-go-s3 | `dropped_references_are_absent` | pass (2/2) | the store lists exactly what retention should have left: retained 4 (history-0011, history-0012, history-0013, history-0014), dropped 11 (history-0000, history-0001, history-0002, history-0003, history-0004, history-0005, history-0006, history-0007, history-0008, history-0009, history-0010); absence was read from the backend's own successful listing, not inferred from a failed command |
| `blob/source-history` | amber-go-s3 | `fresh_pull_restores_correctly` | pass (2/2) | 11 of the 11 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/source-history` | amber-go-s3 | `incremental_pull_restores_correctly` | pass (2/2) | 15 of the 15 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/source-history` | amber-go-s3 | `missing_object_is_detected` | pass (2/2) | after deleting source-history/amber-go-s3/rep0/store/packstore/0000000000000001.seg, the backend failed rather than returning an incomplete result: /nix/store/y73dri4kbmb0jx08kdi93cyyxphjk2d6-amber-store-go-0.0.7-4ed4660657b1/bin/amber-store --store <scratch>/runs/blob-source-history-amber-go-s3-rep0/pulled-probe --segment-size 8388608 export ref:history-0011: exited 1 |
| `blob/source-history` | amber-go-s3 | `retained_content_valid_after_cleanup` | pass (2/2) | 4 of the 4 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/source-history` | amber-rust-s3 | `dropped_references_are_absent` | pass (2/2) | the store lists exactly what retention should have left: retained 4 (history-0011, history-0012, history-0013, history-0014), dropped 11 (history-0000, history-0001, history-0002, history-0003, history-0004, history-0005, history-0006, history-0007, history-0008, history-0009, history-0010); absence was read from the backend's own successful listing, not inferred from a failed command |
| `blob/source-history` | amber-rust-s3 | `fresh_pull_restores_correctly` | pass (2/2) | 11 of the 11 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/source-history` | amber-rust-s3 | `incremental_pull_restores_correctly` | pass (2/2) | 15 of the 15 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/source-history` | amber-rust-s3 | `missing_object_is_detected` | pass (2/2) | after deleting source-history/amber-rust-s3/rep0/store/refs/refs.redb, the backend failed rather than returning an incomplete result: /nix/store/5cqjvnw3zg461xww08q9gj3lwd8v2iz1-amber-store-rust-0.2.0-141df2b0a9a8/bin/amber-store --store <scratch>/runs/blob-source-history-amber-rust-s3-rep0/pulled-probe --segment-size 8388608 export ref:history-0011: exited 1 |
| `blob/source-history` | amber-rust-s3 | `retained_content_valid_after_cleanup` | pass (2/2) | 4 of the 4 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/source-history` | git-bundle-s3 | `fresh_pull_matches_source` | pass (2/2) | the destination repository passed `git fsck`, holds every branch and tag of the source at the same commit id, and a working tree checked out of it matches the manifest of the delivered history state 10 |
| `blob/source-history` | git-bundle-s3 | `incremental_pull_matches_source` | pass (2/2) | the increment applied to the existing clone, which passed `git fsck`, and a working tree checked out of its main tip matches the manifest of history state 14 |
| `blob/source-history` | git-bundle-s3 | `missing_object_is_detected` | pass (2/2) | after deleting source-history/git-bundle-s3/rep0/bundles/current.bundle, the backend failed rather than returning an incomplete result: /nix/store/0fid83wpnczqbql3y712a5p6f07kzc1p-minio-client-2025-08-13T08-35-41Z/bin/mc --config-dir <scratch>/mc-config --no-color mirror --overwrite bench/amber-cas-bench/bench/smoke-1789241936307339195/source-history/git-bundle-s3/rep0/bundles <scratch>/runs/blob-source-history-git-bundle-s3-rep0/download-probe: exited 1 |
| `blob/source-history` | git-bundle-s3 | `retained_content_valid_after_cleanup` | pass (2/2) | the destination repository passed `git fsck`, holds every branch and tag of the source at the same commit id, and a working tree checked out of it matches the manifest of the delivered history state 14 |
| `blob/source-history` | git-bundle-s3 | `superseded_objects_are_absent` | pass (2/2) | the bucket holds the consolidated bundle and none of the superseded ones (1 object(s) remain) |
| `git/history` | amber-go | `all_retained_versions_match` | pass (2/2) | every one of the 15 retained versions was restored and compared byte for byte; 0 unverified |
| `git/history` | amber-go | `head_version_matches_manifest` | pass (2/2) | 197 entries expected under git-amber-go-rep0/checkout-head; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `git/history` | amber-rust | `all_retained_versions_match` | pass (2/2) | every one of the 15 retained versions was restored and compared byte for byte; 0 unverified |
| `git/history` | amber-rust | `head_version_matches_manifest` | pass (2/2) | 197 entries expected under git-amber-rust-rep0/checkout-head; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `git/history` | git | `all_retained_versions_match` | pass (2/2) | every one of the 15 retained versions was restored and compared byte for byte; 0 unverified |
| `git/history` | git | `clone_matches_source` | pass (2/2) | the destination repository passed `git fsck`, holds every branch and tag of the source at the same commit id, and a working tree checked out of it matches the manifest of history state 9 |
| `git/history` | git | `head_version_matches_manifest` | pass (2/2) | 197 entries expected under git-git-rep0/checkout-head; 0 missing, 0 extra, 0 differing (mode=ExecBit, mtime=false, symlinks=true) |
| `git/history` | git | `incremental_fetch_matches_source` | pass (2/2) | the destination repository passed `git fsck`, holds every branch and tag of the source at the same commit id, and a working tree checked out of it matches the manifest of history state 14 |
| `nix/closure` | amber-go | `closure_is_complete` | pass (2/2) | every reference of every retained path is itself retained. These cores store trees and have no notion of references between roots, so this was checked by the harness against the graph recorded from the source store, not by the store under test. |
| `nix/closure` | amber-go | `closure_matches_source` | pass (2/2) | all 9 store paths of generation 2 came back with identical bytes, permissions, symlink targets and timestamps, and the destination holds nothing else |
| `nix/closure` | amber-go | `nar_hashes_match_recomputed` | pass (2/2) | the NAR hash recomputed with `nix hash path` over every one of the 9 restored store paths equals the hash the source Nix store recorded. These cores register no NAR hashes themselves — see the unsupported nar_and_reference_registration operation — so this was recomputed from the restored bytes. |
| `nix/closure` | amber-go | `post_gc_closure_is_complete` | pass (2/2) | every reference of every retained path is itself retained. These cores store trees and have no notion of references between roots, so this was checked by the harness against the graph recorded from the source store, not by the store under test. |
| `nix/closure` | amber-go | `post_gc_closure_matches_source` | pass (2/2) | all 9 store paths of generation 2 came back with identical bytes, permissions, symlink targets and timestamps, and the destination holds nothing else |
| `nix/closure` | amber-go | `post_gc_nar_hashes_match_recomputed` | pass (2/2) | the NAR hash recomputed with `nix hash path` over every one of the 9 restored store paths equals the hash the source Nix store recorded. These cores register no NAR hashes themselves — see the unsupported nar_and_reference_registration operation — so this was recomputed from the restored bytes. |
| `nix/closure` | amber-rust | `closure_is_complete` | pass (2/2) | every reference of every retained path is itself retained. These cores store trees and have no notion of references between roots, so this was checked by the harness against the graph recorded from the source store, not by the store under test. |
| `nix/closure` | amber-rust | `closure_matches_source` | pass (2/2) | all 9 store paths of generation 2 came back with identical bytes, permissions, symlink targets and timestamps, and the destination holds nothing else |
| `nix/closure` | amber-rust | `nar_hashes_match_recomputed` | pass (2/2) | the NAR hash recomputed with `nix hash path` over every one of the 9 restored store paths equals the hash the source Nix store recorded. These cores register no NAR hashes themselves — see the unsupported nar_and_reference_registration operation — so this was recomputed from the restored bytes. |
| `nix/closure` | amber-rust | `post_gc_closure_is_complete` | pass (2/2) | every reference of every retained path is itself retained. These cores store trees and have no notion of references between roots, so this was checked by the harness against the graph recorded from the source store, not by the store under test. |
| `nix/closure` | amber-rust | `post_gc_closure_matches_source` | pass (2/2) | all 9 store paths of generation 2 came back with identical bytes, permissions, symlink targets and timestamps, and the destination holds nothing else |
| `nix/closure` | amber-rust | `post_gc_nar_hashes_match_recomputed` | pass (2/2) | the NAR hash recomputed with `nix hash path` over every one of the 9 restored store paths equals the hash the source Nix store recorded. These cores register no NAR hashes themselves — see the unsupported nar_and_reference_registration operation — so this was recomputed from the restored bytes. |
| `nix/closure` | nix | `closure_is_complete` | pass (2/2) | every reference of every retained path is itself retained; Nix enforces this itself |
| `nix/closure` | nix | `closure_matches_source` | pass (2/2) | all 9 store paths of generation 2 came back with identical bytes, permissions, symlink targets and timestamps, and the destination holds nothing else |
| `nix/closure` | nix | `nar_hashes_and_references_match` | pass (2/2) | all 9 paths of the closure are registered in the destination store with the source store's NAR hash and reference list |
| `nix/closure` | nix | `post_gc_closure_is_complete` | pass (2/2) | every reference of every retained path is itself retained; Nix enforces this itself |
| `nix/closure` | nix | `post_gc_closure_matches_source` | pass (2/2) | all 9 store paths of generation 2 came back with identical bytes, permissions, symlink targets and timestamps, and the destination holds nothing else |
| `nix/closure` | nix | `post_gc_nar_hashes_and_references_match` | pass (2/2) | all 9 paths of the closure are registered in the destination store with the source store's NAR hash and reference list |
| `tree/lifecycle` | amber-go | `integrity_check` | pass (2/2) | the backend's own consistency check passed over the whole store |
| `tree/lifecycle` | amber-go | `post_gc_restore_matches_manifest` | pass (2/2) | 284 entries expected under tree-amber-go-rep0/restore-gen1; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | amber-go | `restore_matches_manifest` | pass (2/2) | 283 entries expected under tree-amber-go-rep0/restore-gen0; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | amber-go | `retention_took_effect` | pass (2/2) | the store lists exactly what retention should have left: retained 1 (gen1), dropped 2 (gen0, gen0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `tree/lifecycle` | amber-rust | `integrity_check` | pass (2/2) | the backend's own consistency check passed over the whole store |
| `tree/lifecycle` | amber-rust | `post_gc_restore_matches_manifest` | pass (2/2) | 284 entries expected under tree-amber-rust-rep0/restore-gen1; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | amber-rust | `restore_matches_manifest` | pass (2/2) | 283 entries expected under tree-amber-rust-rep0/restore-gen0; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | amber-rust | `retention_took_effect` | pass (2/2) | the store lists exactly what retention should have left: retained 1 (gen1), dropped 2 (gen0, gen0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `tree/lifecycle` | fs-sha256 | `integrity_check` | pass (2/2) | the backend's own consistency check passed over the whole store |
| `tree/lifecycle` | fs-sha256 | `post_gc_restore_matches_manifest` | pass (2/2) | 284 entries expected under tree-fs-sha256-rep0/restore-gen1; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | fs-sha256 | `restore_matches_manifest` | pass (2/2) | 283 entries expected under tree-fs-sha256-rep0/restore-gen0; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | fs-sha256 | `retention_took_effect` | pass (2/2) | the store lists exactly what retention should have left: retained 1 (gen1), dropped 2 (gen0, gen0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `tree/lifecycle` | git | `integrity_check` | pass (2/2) | the backend's own consistency check passed over the whole store |
| `tree/lifecycle` | git | `post_gc_restore_matches_manifest` | pass (2/2) | 284 entries expected under tree-git-rep0/restore-gen1; 0 missing, 0 extra, 0 differing (mode=ExecBit, mtime=false, symlinks=true) |
| `tree/lifecycle` | git | `restore_matches_manifest` | pass (2/2) | 283 entries expected under tree-git-rep0/restore-gen0; 0 missing, 0 extra, 0 differing (mode=ExecBit, mtime=false, symlinks=true) |
| `tree/lifecycle` | git | `retention_took_effect` | pass (2/2) | the store lists exactly what retention should have left: retained 1 (gen1), dropped 2 (gen0, gen0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `tree/lifecycle` | restic | `integrity_check` | pass (2/2) | the backend's own consistency check passed over the whole store |
| `tree/lifecycle` | restic | `post_gc_restore_matches_manifest` | pass (2/2) | 284 entries expected under tree-restic-rep0/restore-gen1; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | restic | `restore_matches_manifest` | pass (2/2) | 283 entries expected under tree-restic-rep0/restore-gen0; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | restic | `retention_took_effect` | pass (2/2) | the store lists exactly what retention should have left: retained 1 (gen1), dropped 2 (gen0, gen0-again); absence was read from the backend's own successful listing, not inferred from a failed command |

## Failures

None.

## Configuration and environment

* Command line: `<repo>/target/release/amber-cas-bench run --harness-repo <repo> --flake-dir <repo> --amber-rust-bin /nix/store/5cqjvnw3zg461xww08q9gj3lwd8v2iz1-amber-store-rust-0.2.0-141df2b0a9a8/bin/amber-store --amber-rust-rev 141df2b0a9a8ad1726769f63811db3a166be188a --amber-go-bin /nix/store/y73dri4kbmb0jx08kdi93cyyxphjk2d6-amber-store-go-0.0.7-4ed4660657b1/bin/amber-store --amber-go-rev 4ed4660657b12421a534ab0b08cfd717ae3d2291 --profile smoke --repeats 2 --seed 20260913 --out <run-output-dir>`
* Harness: version 0.1.0, commit `3656de6abe68de2e52a52bde2fb533afefa6730d`
* Chunking, identical for both Amber cores: min 32.00 KiB, average 512.00 KiB, max 1.00 MiB, item-bits 7, xattr-inline-max 256
* Pack segment size, identical for both Amber cores: 8.00 MiB (the cores default to 2 GiB; a smaller, explicit value is used here because a segment is the unit collection reaps, and a profile that stores less than one segment could never reclaim anything)
* Corpus: seed 20260913, 3 generation(s), 32.65 MiB logical bytes in generation 0, manifest digest `6194ca3bd22b02ffef9af465ed3482162f74ea58700915535cd1eb2487552355`
* Source history: 15 commits on 3 branches with 2 tags
* Nix closures: the deterministic fixture closure built by this flake (flake output `nix-fixtures-smoke`). Generation 1 closure 8 paths, generation 2 9 paths, 6 shared
* Object store: local garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] started by the harness
* Gateway: every backend's S3 traffic is forwarded verbatim through a counting gateway at http://127.0.0.1:39091; requests are counted by method and bodies by direction, including retries. The harness's own listing and cleanup calls bypass it.
* Network shaping: none: the loopback path to the local object store was measured as it is
* Bucket `amber-cas-bench`, prefix `bench/smoke-1789241936307339195`, region `garage`, remote: false. not recorded: access key and secret are passed to child processes in the environment and never written to any output file
* Execution order was randomised per repetition with seed `6624854655743516624`; the exact order is in `report.json` under `execution_order`.
* Host: linux x86_64 on AMD Ryzen 9 7950X3D 16-Core Processor, 32 logical core(s), 125.32 GiB of memory
* Kernel: 6.18.45 #1-NixOS SMP PREEMPT_DYNAMIC Wed Aug 19 16:18:21 UTC 2026
* Load average when the run started: 2.81 2.42 2.40 (1, 5, 15 minutes)
* Scratch: `<scratch>`, 172.22 GiB free at the start

### Executables measured

| tool | version | path | sha256 | source commit |
|---|---|---|---|---|
| amber-go | not reported by this executable | `/nix/store/y73dri4kbmb0jx08kdi93cyyxphjk2d6-amber-store-go-0.0.7-4ed4660657b1/bin/amber-store` | `18f944b7c8a8ead8` | `4ed4660657b1` |
| amber-rust | not reported by this executable | `/nix/store/5cqjvnw3zg461xww08q9gj3lwd8v2iz1-amber-store-rust-0.2.0-141df2b0a9a8/bin/amber-store` | `0d86e768712cce7f` | `141df2b0a9a8` |
| garage | garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] | `/nix/store/7xgk0mkgkmi7x430hm6gwga8afm312bq-garage-1.3.1/bin/garage` | `2e6718fa730aab04` | — |
| git | git version 2.54.0 | `/nix/store/5vpcfsdx1lcnlb2y4k7xyc74shp5qwbh-git-2.54.0/bin/git` | `27232b9fd6b101c5` | — |
| mc | mc version RELEASE.2025-08-13T08-35-41Z (commit-id=RELEASE.2025-08-13T08-35-41Z) | `/nix/store/0fid83wpnczqbql3y712a5p6f07kzc1p-minio-client-2025-08-13T08-35-41Z/bin/mc` | `6476e69f864a45f0` | — |
| nix | nix (Nix) 2.34.8 | `/nix/store/0djmqy9c50czxaakvi7xqinz30wlbs6g-nix-2.34.8/bin/nix` | `d9a869cc38d8697b` | — |
| restic | restic 0.18.1 compiled with go1.26.7 on linux/amd64 | `/nix/store/jh7vrxar93sk4m8igr35zdlgrz3nakrk-restic-0.18.1/bin/restic` | `b4d2faf4675fd550` | — |

