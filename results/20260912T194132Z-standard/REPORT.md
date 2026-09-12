# Amber-Store CAS comparison benchmarks

Run `standard-1789242092913944754`, profile **standard**, seed `20260912`, 3 repetition(s), 32 worker(s).

~2.1 GiB per corpus generation, 3 generations, 51 commits, the 180 MiB Nix fixture closure, 3 repetitions by default. Hours.

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

| operation | amber-rust-s3 | nix-binary-cache | amber-go-s3 |
|---|---|---|---|
| `push_initial` | 189.7 ms | 19.48 s | 191.8 ms |
| `push_noop` | 19.2 ms | 21.4 ms | 19.9 ms |
| `pull_fresh` | 69.9 ms | 150.4 ms | 72.7 ms |
| `push_incremental` | 174.3 ms | 3.03 s | 178.8 ms |
| `pull_incremental` | 65.8 ms | 128.8 ms | 71.2 ms |
| `retention_cleanup` | 196.4 ms | 37.8 ms | 200.1 ms |

### Throughput over the logical bytes processed (median)

| operation | amber-rust-s3 | nix-binary-cache | amber-go-s3 |
|---|---|---|---|
| `push_initial` | 568.44 MiB/s | 5.34 MiB/s | 544.09 MiB/s |
| `push_noop` | 151.47 KiB/s | 0 B/s | 335.50 KiB/s |
| `pull_fresh` | 1.50 GiB/s | 691.85 MiB/s | 1.40 GiB/s |
| `push_incremental` | 342.71 MiB/s | 5.28 MiB/s | 314.41 MiB/s |
| `pull_incremental` | 906.89 MiB/s | 124.39 MiB/s | 787.85 MiB/s |
| `retention_cleanup` | 630.98 MiB/s | 206.98 KiB/s | 601.90 MiB/s |

### CPU time, user + system (median)

| operation | amber-rust-s3 | nix-binary-cache | amber-go-s3 |
|---|---|---|---|
| `push_initial` | 145.8 ms | 28.13 s | 157.8 ms |
| `push_noop` | 25.0 ms | 18.9 ms | 27.9 ms |
| `pull_fresh` | 71.3 ms | 303.7 ms | 81.0 ms |
| `push_incremental` | 86.8 ms | 3.76 s | 97.4 ms |
| `pull_incremental` | 60.1 ms | 147.3 ms | 70.4 ms |
| `retention_cleanup` | 165.7 ms | 49.2 ms | 174.3 ms |

### Peak resident set size (median)

| operation | amber-rust-s3 | nix-binary-cache | amber-go-s3 |
|---|---|---|---|
| `push_initial` | 83.45 MiB | 393.98 MiB | 85.70 MiB |
| `push_noop` | 83.45 MiB | 84.45 MiB | 85.70 MiB |
| `pull_fresh` | 83.45 MiB | 84.45 MiB | 85.70 MiB |
| `push_incremental` | 83.45 MiB | 179.73 MiB | 85.70 MiB |
| `pull_incremental` | 83.45 MiB | 84.45 MiB | 85.70 MiB |
| `retention_cleanup` | 83.45 MiB | 84.45 MiB | 85.70 MiB |

### Dispersion of elapsed time

| backend | operation | n | median | p95 | min | max | stddev | cv |
|---|---|---|---|---|---|---|---|---|
| amber-rust-s3 | `push_initial` | 3 | 189.7 ms | 193.3 ms | 189.0 ms | 193.3 ms | 2.3 ms | 1.2% |
| amber-rust-s3 | `push_noop` | 3 | 19.2 ms | 20.5 ms | 18.7 ms | 20.5 ms | 930.8 µs | 4.8% |
| amber-rust-s3 | `pull_fresh` | 3 | 69.9 ms | 74.2 ms | 65.9 ms | 74.2 ms | 4.2 ms | 6.0% |
| amber-rust-s3 | `push_incremental` | 3 | 174.3 ms | 176.6 ms | 170.4 ms | 176.6 ms | 3.1 ms | 1.8% |
| amber-rust-s3 | `pull_incremental` | 3 | 65.8 ms | 103.1 ms | 65.5 ms | 103.1 ms | 21.6 ms | 27.7% |
| amber-rust-s3 | `retention_cleanup` | 3 | 196.4 ms | 199.0 ms | 195.6 ms | 199.0 ms | 1.7 ms | 0.9% |
| nix-binary-cache | `push_initial` | 3 | 19.48 s | 19.52 s | 19.38 s | 19.52 s | 73.3 ms | 0.4% |
| nix-binary-cache | `push_noop` | 3 | 21.4 ms | 21.5 ms | 21.3 ms | 21.5 ms | 105.2 µs | 0.5% |
| nix-binary-cache | `pull_fresh` | 3 | 150.4 ms | 163.4 ms | 146.1 ms | 163.4 ms | 9.0 ms | 5.9% |
| nix-binary-cache | `push_incremental` | 3 | 3.03 s | 3.27 s | 3.02 s | 3.27 s | 140.8 ms | 4.5% |
| nix-binary-cache | `pull_incremental` | 3 | 128.8 ms | 130.3 ms | 128.0 ms | 130.3 ms | 1.2 ms | 0.9% |
| nix-binary-cache | `retention_cleanup` | 3 | 37.8 ms | 38.0 ms | 37.2 ms | 38.0 ms | 409.4 µs | 1.1% |
| amber-go-s3 | `push_initial` | 3 | 191.8 ms | 194.2 ms | 187.5 ms | 194.2 ms | 3.4 ms | 1.8% |
| amber-go-s3 | `push_noop` | 3 | 19.9 ms | 20.4 ms | 18.9 ms | 20.4 ms | 781.0 µs | 4.0% |
| amber-go-s3 | `pull_fresh` | 3 | 72.7 ms | 74.8 ms | 71.9 ms | 74.8 ms | 1.5 ms | 2.1% |
| amber-go-s3 | `push_incremental` | 3 | 178.8 ms | 179.0 ms | 176.1 ms | 179.0 ms | 1.6 ms | 0.9% |
| amber-go-s3 | `pull_incremental` | 3 | 71.2 ms | 109.9 ms | 62.6 ms | 109.9 ms | 25.1 ms | 31.0% |
| amber-go-s3 | `retention_cleanup` | 3 | 200.1 ms | 204.1 ms | 198.6 ms | 204.1 ms | 2.8 ms | 1.4% |

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
| amber-rust-s3 | `push_initial` | `s3_bytes_down` | 4.02 KiB |
| amber-rust-s3 | `push_initial` | `s3_bytes_up` | 107.83 MiB |
| amber-rust-s3 | `push_initial` | `s3_connections` | 8 |
| amber-rust-s3 | `push_initial` | `s3_request_body_bytes` | 107.82 MiB |
| amber-rust-s3 | `push_initial` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_initial` | `s3_requests_POST` | 4 |
| amber-rust-s3 | `push_initial` | `s3_requests_PUT` | 9 |
| amber-rust-s3 | `push_initial` | `s3_requests_total` | 15 |
| amber-rust-s3 | `push_initial` | `s3_response_body_bytes` | 2.39 KiB |
| amber-rust-s3 | `push_initial` | `s3_responses_2xx` | 14 |
| amber-rust-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_initial` | `stored_bytes` | 107.67 MiB |
| amber-rust-s3 | `push_initial` | `stored_objects` | 3 |
| amber-rust-s3 | `push_noop` | `s3_bytes_down` | 1.75 KiB |
| amber-rust-s3 | `push_noop` | `s3_bytes_up` | 1.17 KiB |
| amber-rust-s3 | `push_noop` | `s3_connections` | 1 |
| amber-rust-s3 | `push_noop` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `push_noop` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_noop` | `s3_requests_total` | 2 |
| amber-rust-s3 | `push_noop` | `s3_response_body_bytes` | 1.52 KiB |
| amber-rust-s3 | `push_noop` | `s3_responses_2xx` | 1 |
| amber-rust-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_noop` | `stored_bytes` | 107.67 MiB |
| amber-rust-s3 | `push_noop` | `stored_objects` | 3 |
| amber-rust-s3 | `pull_fresh` | `delivered_logical_bytes` | 176.00 MiB |
| amber-rust-s3 | `pull_fresh` | `delivered_references` | 8 |
| amber-rust-s3 | `pull_fresh` | `s3_bytes_down` | 107.68 MiB |
| amber-rust-s3 | `pull_fresh` | `s3_bytes_up` | 5.03 KiB |
| amber-rust-s3 | `pull_fresh` | `s3_connections` | 3 |
| amber-rust-s3 | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `pull_fresh` | `s3_requests_GET` | 7 |
| amber-rust-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| amber-rust-s3 | `pull_fresh` | `s3_requests_total` | 8 |
| amber-rust-s3 | `pull_fresh` | `s3_response_body_bytes` | 107.68 MiB |
| amber-rust-s3 | `pull_fresh` | `s3_responses_2xx` | 6 |
| amber-rust-s3 | `pull_fresh` | `s3_responses_4xx` | 2 |
| amber-rust-s3 | `pull_fresh` | `stored_bytes` | 107.67 MiB |
| amber-rust-s3 | `pull_fresh` | `stored_objects` | 3 |
| amber-rust-s3 | `push_incremental` | `s3_bytes_down` | 3.45 KiB |
| amber-rust-s3 | `push_incremental` | `s3_bytes_up` | 59.73 MiB |
| amber-rust-s3 | `push_incremental` | `s3_connections` | 5 |
| amber-rust-s3 | `push_incremental` | `s3_request_body_bytes` | 59.73 MiB |
| amber-rust-s3 | `push_incremental` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_requests_POST` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_requests_PUT` | 5 |
| amber-rust-s3 | `push_incremental` | `s3_requests_total` | 9 |
| amber-rust-s3 | `push_incremental` | `s3_response_body_bytes` | 2.43 KiB |
| amber-rust-s3 | `push_incremental` | `s3_responses_2xx` | 8 |
| amber-rust-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_incremental` | `stored_bytes` | 123.69 MiB |
| amber-rust-s3 | `push_incremental` | `stored_objects` | 3 |
| amber-rust-s3 | `pull_incremental` | `delivered_logical_bytes` | 184.00 MiB |
| amber-rust-s3 | `pull_incremental` | `delivered_references` | 9 |
| amber-rust-s3 | `pull_incremental` | `s3_bytes_down` | 59.65 MiB |
| amber-rust-s3 | `pull_incremental` | `s3_bytes_up` | 4.43 KiB |
| amber-rust-s3 | `pull_incremental` | `s3_connections` | 2 |
| amber-rust-s3 | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `pull_incremental` | `s3_requests_GET` | 6 |
| amber-rust-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| amber-rust-s3 | `pull_incremental` | `s3_requests_total` | 7 |
| amber-rust-s3 | `pull_incremental` | `s3_response_body_bytes` | 59.64 MiB |
| amber-rust-s3 | `pull_incremental` | `s3_responses_2xx` | 5 |
| amber-rust-s3 | `pull_incremental` | `s3_responses_4xx` | 2 |
| amber-rust-s3 | `pull_incremental` | `stored_bytes` | 123.69 MiB |
| amber-rust-s3 | `pull_incremental` | `stored_objects` | 3 |
| amber-rust-s3 | `retention_cleanup` | `retained_logical_bytes` | 184.00 MiB |
| amber-rust-s3 | `retention_cleanup` | `retained_references` | 9 |
| amber-rust-s3 | `retention_cleanup` | `s3_bytes_down` | 6.12 KiB |
| amber-rust-s3 | `retention_cleanup` | `s3_bytes_up` | 123.91 MiB |
| amber-rust-s3 | `retention_cleanup` | `s3_connections` | 9 |
| amber-rust-s3 | `retention_cleanup` | `s3_request_body_bytes` | 123.90 MiB |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_POST` | 6 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_PUT` | 10 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_total` | 18 |
| amber-rust-s3 | `retention_cleanup` | `s3_response_body_bytes` | 4.17 KiB |
| amber-rust-s3 | `retention_cleanup` | `s3_responses_2xx` | 17 |
| amber-rust-s3 | `retention_cleanup` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `retention_cleanup` | `stored_bytes` | 123.73 MiB |
| amber-rust-s3 | `retention_cleanup` | `stored_objects` | 3 |
| nix-binary-cache | `push_initial` | `s3_bytes_down` | 7.88 KiB |
| nix-binary-cache | `push_initial` | `s3_bytes_up` | 104.05 MiB |
| nix-binary-cache | `push_initial` | `s3_connections` | 8 |
| nix-binary-cache | `push_initial` | `s3_request_body_bytes` | 104.03 MiB |
| nix-binary-cache | `push_initial` | `s3_requests_GET` | 9 |
| nix-binary-cache | `push_initial` | `s3_requests_HEAD` | 8 |
| nix-binary-cache | `push_initial` | `s3_requests_PUT` | 17 |
| nix-binary-cache | `push_initial` | `s3_requests_total` | 34 |
| nix-binary-cache | `push_initial` | `s3_response_body_bytes` | 2.46 KiB |
| nix-binary-cache | `push_initial` | `s3_responses_1xx` | 7 |
| nix-binary-cache | `push_initial` | `s3_responses_2xx` | 17 |
| nix-binary-cache | `push_initial` | `s3_responses_4xx` | 17 |
| nix-binary-cache | `push_initial` | `stored_bytes` | 104.03 MiB |
| nix-binary-cache | `push_initial` | `stored_objects` | 17 |
| nix-binary-cache | `push_noop` | `s3_bytes_down` | 0 B |
| nix-binary-cache | `push_noop` | `s3_bytes_up` | 0 B |
| nix-binary-cache | `push_noop` | `s3_connections` | 0 |
| nix-binary-cache | `push_noop` | `s3_request_body_bytes` | 0 B |
| nix-binary-cache | `push_noop` | `s3_requests_total` | 0 |
| nix-binary-cache | `push_noop` | `s3_response_body_bytes` | 0 B |
| nix-binary-cache | `push_noop` | `stored_bytes` | 104.03 MiB |
| nix-binary-cache | `push_noop` | `stored_objects` | 17 |
| nix-binary-cache | `pull_fresh` | `delivered_logical_bytes` | 176.00 MiB |
| nix-binary-cache | `pull_fresh` | `delivered_references` | 8 |
| nix-binary-cache | `pull_fresh` | `s3_bytes_down` | 104.03 MiB |
| nix-binary-cache | `pull_fresh` | `s3_bytes_up` | 12.11 KiB |
| nix-binary-cache | `pull_fresh` | `s3_connections` | 5 |
| nix-binary-cache | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| nix-binary-cache | `pull_fresh` | `s3_requests_GET` | 17 |
| nix-binary-cache | `pull_fresh` | `s3_requests_HEAD` | 3 |
| nix-binary-cache | `pull_fresh` | `s3_requests_total` | 20 |
| nix-binary-cache | `pull_fresh` | `s3_response_body_bytes` | 104.03 MiB |
| nix-binary-cache | `pull_fresh` | `s3_responses_2xx` | 20 |
| nix-binary-cache | `pull_fresh` | `stored_bytes` | 104.03 MiB |
| nix-binary-cache | `pull_fresh` | `stored_objects` | 17 |
| nix-binary-cache | `push_incremental` | `s3_bytes_down` | 2.73 KiB |
| nix-binary-cache | `push_incremental` | `s3_bytes_up` | 16.02 MiB |
| nix-binary-cache | `push_incremental` | `s3_connections` | 3 |
| nix-binary-cache | `push_incremental` | `s3_request_body_bytes` | 16.01 MiB |
| nix-binary-cache | `push_incremental` | `s3_requests_GET` | 3 |
| nix-binary-cache | `push_incremental` | `s3_requests_HEAD` | 3 |
| nix-binary-cache | `push_incremental` | `s3_requests_PUT` | 6 |
| nix-binary-cache | `push_incremental` | `s3_requests_total` | 12 |
| nix-binary-cache | `push_incremental` | `s3_response_body_bytes` | 849 B |
| nix-binary-cache | `push_incremental` | `s3_responses_1xx` | 2 |
| nix-binary-cache | `push_incremental` | `s3_responses_2xx` | 6 |
| nix-binary-cache | `push_incremental` | `s3_responses_4xx` | 6 |
| nix-binary-cache | `push_incremental` | `stored_bytes` | 120.04 MiB |
| nix-binary-cache | `push_incremental` | `stored_objects` | 23 |
| nix-binary-cache | `pull_incremental` | `delivered_logical_bytes` | 184.00 MiB |
| nix-binary-cache | `pull_incremental` | `delivered_references` | 9 |
| nix-binary-cache | `pull_incremental` | `s3_bytes_down` | 16.01 MiB |
| nix-binary-cache | `pull_incremental` | `s3_bytes_up` | 1.86 KiB |
| nix-binary-cache | `pull_incremental` | `s3_connections` | 2 |
| nix-binary-cache | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| nix-binary-cache | `pull_incremental` | `s3_requests_GET` | 3 |
| nix-binary-cache | `pull_incremental` | `s3_requests_total` | 3 |
| nix-binary-cache | `pull_incremental` | `s3_response_body_bytes` | 16.01 MiB |
| nix-binary-cache | `pull_incremental` | `s3_responses_2xx` | 3 |
| nix-binary-cache | `pull_incremental` | `stored_bytes` | 120.04 MiB |
| nix-binary-cache | `pull_incremental` | `stored_objects` | 23 |
| nix-binary-cache | `retention_cleanup` | `retained_logical_bytes` | 184.00 MiB |
| nix-binary-cache | `retention_cleanup` | `retained_references` | 9 |
| nix-binary-cache | `retention_cleanup` | `s3_bytes_down` | 2.70 KiB |
| nix-binary-cache | `retention_cleanup` | `s3_bytes_up` | 5.11 KiB |
| nix-binary-cache | `retention_cleanup` | `s3_connections` | 2 |
| nix-binary-cache | `retention_cleanup` | `s3_request_body_bytes` | 360 B |
| nix-binary-cache | `retention_cleanup` | `s3_requests_GET` | 2 |
| nix-binary-cache | `retention_cleanup` | `s3_requests_HEAD` | 4 |
| nix-binary-cache | `retention_cleanup` | `s3_requests_POST` | 2 |
| nix-binary-cache | `retention_cleanup` | `s3_requests_total` | 8 |
| nix-binary-cache | `retention_cleanup` | `s3_response_body_bytes` | 1.40 KiB |
| nix-binary-cache | `retention_cleanup` | `s3_responses_2xx` | 6 |
| nix-binary-cache | `retention_cleanup` | `s3_responses_4xx` | 2 |
| nix-binary-cache | `retention_cleanup` | `stored_bytes` | 120.04 MiB |
| nix-binary-cache | `retention_cleanup` | `stored_objects` | 21 |
| amber-go-s3 | `push_initial` | `s3_bytes_down` | 6.56 KiB |
| amber-go-s3 | `push_initial` | `s3_bytes_up` | 104.34 MiB |
| amber-go-s3 | `push_initial` | `s3_connections` | 14 |
| amber-go-s3 | `push_initial` | `s3_request_body_bytes` | 104.32 MiB |
| amber-go-s3 | `push_initial` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_initial` | `s3_requests_POST` | 4 |
| amber-go-s3 | `push_initial` | `s3_requests_PUT` | 22 |
| amber-go-s3 | `push_initial` | `s3_requests_total` | 28 |
| amber-go-s3 | `push_initial` | `s3_response_body_bytes` | 2.38 KiB |
| amber-go-s3 | `push_initial` | `s3_responses_2xx` | 27 |
| amber-go-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_initial` | `stored_bytes` | 104.17 MiB |
| amber-go-s3 | `push_initial` | `stored_objects` | 16 |
| amber-go-s3 | `push_noop` | `s3_bytes_down` | 5.52 KiB |
| amber-go-s3 | `push_noop` | `s3_bytes_up` | 1.17 KiB |
| amber-go-s3 | `push_noop` | `s3_connections` | 1 |
| amber-go-s3 | `push_noop` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `push_noop` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_noop` | `s3_requests_total` | 2 |
| amber-go-s3 | `push_noop` | `s3_response_body_bytes` | 5.30 KiB |
| amber-go-s3 | `push_noop` | `s3_responses_2xx` | 1 |
| amber-go-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_noop` | `stored_bytes` | 104.17 MiB |
| amber-go-s3 | `push_noop` | `stored_objects` | 16 |
| amber-go-s3 | `pull_fresh` | `delivered_logical_bytes` | 176.00 MiB |
| amber-go-s3 | `pull_fresh` | `delivered_references` | 8 |
| amber-go-s3 | `pull_fresh` | `s3_bytes_down` | 104.18 MiB |
| amber-go-s3 | `pull_fresh` | `s3_bytes_up` | 12.70 KiB |
| amber-go-s3 | `pull_fresh` | `s3_connections` | 15 |
| amber-go-s3 | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `pull_fresh` | `s3_requests_GET` | 20 |
| amber-go-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| amber-go-s3 | `pull_fresh` | `s3_requests_total` | 21 |
| amber-go-s3 | `pull_fresh` | `s3_response_body_bytes` | 104.18 MiB |
| amber-go-s3 | `pull_fresh` | `s3_responses_2xx` | 19 |
| amber-go-s3 | `pull_fresh` | `s3_responses_4xx` | 2 |
| amber-go-s3 | `pull_fresh` | `stored_bytes` | 104.17 MiB |
| amber-go-s3 | `pull_fresh` | `stored_objects` | 16 |
| amber-go-s3 | `push_incremental` | `s3_bytes_down` | 15.71 KiB |
| amber-go-s3 | `push_incremental` | `s3_bytes_up` | 56.21 MiB |
| amber-go-s3 | `push_incremental` | `s3_connections` | 14 |
| amber-go-s3 | `push_incremental` | `s3_request_body_bytes` | 56.19 MiB |
| amber-go-s3 | `push_incremental` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_incremental` | `s3_requests_POST` | 14 |
| amber-go-s3 | `push_incremental` | `s3_requests_PUT` | 16 |
| amber-go-s3 | `push_incremental` | `s3_requests_total` | 32 |
| amber-go-s3 | `push_incremental` | `s3_response_body_bytes` | 11.26 KiB |
| amber-go-s3 | `push_incremental` | `s3_responses_2xx` | 31 |
| amber-go-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_incremental` | `stored_bytes` | 120.19 MiB |
| amber-go-s3 | `push_incremental` | `stored_objects` | 15 |
| amber-go-s3 | `pull_incremental` | `delivered_logical_bytes` | 184.00 MiB |
| amber-go-s3 | `pull_incremental` | `delivered_references` | 9 |
| amber-go-s3 | `pull_incremental` | `s3_bytes_down` | 56.12 MiB |
| amber-go-s3 | `pull_incremental` | `s3_bytes_up` | 10.90 KiB |
| amber-go-s3 | `pull_incremental` | `s3_connections` | 7 |
| amber-go-s3 | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `pull_incremental` | `s3_requests_GET` | 17 |
| amber-go-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| amber-go-s3 | `pull_incremental` | `s3_requests_total` | 18 |
| amber-go-s3 | `pull_incremental` | `s3_response_body_bytes` | 56.11 MiB |
| amber-go-s3 | `pull_incremental` | `s3_responses_2xx` | 16 |
| amber-go-s3 | `pull_incremental` | `s3_responses_4xx` | 2 |
| amber-go-s3 | `pull_incremental` | `stored_bytes` | 120.19 MiB |
| amber-go-s3 | `pull_incremental` | `stored_objects` | 15 |
| amber-go-s3 | `retention_cleanup` | `retained_logical_bytes` | 184.00 MiB |
| amber-go-s3 | `retention_cleanup` | `retained_references` | 9 |
| amber-go-s3 | `retention_cleanup` | `s3_bytes_down` | 16.58 KiB |
| amber-go-s3 | `retention_cleanup` | `s3_bytes_up` | 120.41 MiB |
| amber-go-s3 | `retention_cleanup` | `s3_connections` | 19 |
| amber-go-s3 | `retention_cleanup` | `s3_request_body_bytes` | 120.39 MiB |
| amber-go-s3 | `retention_cleanup` | `s3_requests_GET` | 2 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_POST` | 17 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_PUT` | 16 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_total` | 35 |
| amber-go-s3 | `retention_cleanup` | `s3_response_body_bytes` | 12.28 KiB |
| amber-go-s3 | `retention_cleanup` | `s3_responses_2xx` | 34 |
| amber-go-s3 | `retention_cleanup` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `retention_cleanup` | `stored_bytes` | 120.22 MiB |
| amber-go-s3 | `retention_cleanup` | `stored_objects` | 10 |

## Scenario `git/history`

### Elapsed time (median, lower is better)

| operation | amber-rust | git | amber-go |
|---|---|---|---|
| `history_store` | 2.28 s | 1.92 s | 8.10 s |
| `clone_full` | unsupported | 6.61 s | unsupported |
| `history_store_incremental` | 782.7 ms | 162.3 ms | 2.72 s |
| `fetch_incremental` | unsupported | 49.2 ms | unsupported |
| `gc` | 49.7 ms | 6.35 s | 97.7 ms |
| `list_head` | 131.8 ms | 2.3 ms | 745.3 ms |
| `read_all_versions` | 1.38 s | 1.84 s | 3.67 s |
| `checkout_version` | 160.5 ms | 91.0 ms | 185.4 ms |
| `ref_delete` | 4.7 ms | 1.5 ms | 29.4 ms |

### Throughput over the logical bytes processed (median)

| operation | amber-rust | git | amber-go |
|---|---|---|---|
| `history_store` | 1.76 GiB/s | 2.09 GiB/s | 508.00 MiB/s |
| `clone_full` | unsupported | — | unsupported |
| `history_store_incremental` | 1.71 GiB/s | 8.25 GiB/s | 504.47 MiB/s |
| `fetch_incremental` | unsupported | — | unsupported |
| `gc` | — | — | — |
| `list_head` | — | — | — |
| `read_all_versions` | 3.88 GiB/s | 2.91 GiB/s | 1.46 GiB/s |
| `checkout_version` | — | — | — |
| `ref_delete` | — | — | — |

### CPU time, user + system (median)

| operation | amber-rust | git | amber-go |
|---|---|---|---|
| `history_store` | 10.20 s | 2.02 s | 2.0 min |
| `clone_full` | unsupported | 7.21 s | unsupported |
| `history_store_incremental` | 3.35 s | 195.2 ms | 39.16 s |
| `fetch_incremental` | unsupported | 75.2 ms | unsupported |
| `gc` | 48.8 ms | 6.67 s | 77.1 ms |
| `list_head` | 122.0 ms | 2.2 ms | 259.4 ms |
| `read_all_versions` | 1.35 s | 1.82 s | 6.15 s |
| `checkout_version` | 159.3 ms | 90.2 ms | 293.9 ms |
| `ref_delete` | 4.3 ms | 1.3 ms | 10.3 ms |

### Peak resident set size (median)

| operation | amber-rust | git | amber-go |
|---|---|---|---|
| `history_store` | 81.45 MiB | 82.95 MiB | 944.07 MiB |
| `clone_full` | unsupported | 248.17 MiB | unsupported |
| `history_store_incremental` | 81.45 MiB | 83.20 MiB | 384.78 MiB |
| `fetch_incremental` | unsupported | 83.20 MiB | unsupported |
| `gc` | 81.45 MiB | 249.95 MiB | 100.58 MiB |
| `list_head` | 81.45 MiB | 83.20 MiB | 83.45 MiB |
| `read_all_versions` | 116.83 MiB | 117.18 MiB | 140.10 MiB |
| `checkout_version` | 116.54 MiB | 114.07 MiB | 132.21 MiB |
| `ref_delete` | 81.45 MiB | 83.45 MiB | 83.45 MiB |

### Storage allocated after each operation (median)

| operation | amber-rust | git | amber-go |
|---|---|---|---|
| `history_store` | 112.22 MiB | 125.39 MiB | 115.17 MiB |
| `clone_full` | unsupported | — | unsupported |
| `history_store_incremental` | 113.02 MiB | 127.11 MiB | 116.01 MiB |
| `fetch_incremental` | unsupported | — | unsupported |
| `gc` | 113.12 MiB | 109.41 MiB | 111.77 MiB |
| `list_head` | — | — | — |
| `read_all_versions` | — | — | — |
| `checkout_version` | — | — | — |
| `ref_delete` | — | — | — |

### Space reclaimed (median; positive means released)

| operation | amber-rust | git | amber-go |
|---|---|---|---|
| `history_store` | — | — | — |
| `clone_full` | unsupported | — | unsupported |
| `history_store_incremental` | — | — | — |
| `fetch_incremental` | unsupported | — | unsupported |
| `gc` | -112.00 KiB | 17.70 MiB | 4.23 MiB |
| `list_head` | — | — | — |
| `read_all_versions` | — | — | — |
| `checkout_version` | — | — | — |
| `ref_delete` | — | — | — |

### Dispersion of elapsed time

| backend | operation | n | median | p95 | min | max | stddev | cv |
|---|---|---|---|---|---|---|---|---|
| amber-rust | `history_store` | 3 | 2.28 s | 2.60 s | 2.24 s | 2.60 s | 198.5 ms | 8.4% |
| amber-rust | `history_store_incremental` | 3 | 782.7 ms | 792.1 ms | 782.0 ms | 792.1 ms | 5.6 ms | 0.7% |
| amber-rust | `gc` | 3 | 49.7 ms | 51.9 ms | 48.1 ms | 51.9 ms | 1.9 ms | 3.9% |
| amber-rust | `list_head` | 3 | 131.8 ms | 132.4 ms | 116.3 ms | 132.4 ms | 9.1 ms | 7.2% |
| amber-rust | `read_all_versions` | 3 | 1.38 s | 1.43 s | 1.32 s | 1.43 s | 51.7 ms | 3.8% |
| amber-rust | `checkout_version` | 3 | 160.5 ms | 162.0 ms | 160.3 ms | 162.0 ms | 929.7 µs | 0.6% |
| amber-rust | `ref_delete` | 3 | 4.7 ms | 4.8 ms | 4.6 ms | 4.8 ms | 98.4 µs | 2.1% |
| git | `history_store` | 3 | 1.92 s | 1.98 s | 1.84 s | 1.98 s | 68.7 ms | 3.6% |
| git | `clone_full` | 3 | 6.61 s | 6.64 s | 6.61 s | 6.64 s | 20.3 ms | 0.3% |
| git | `history_store_incremental` | 3 | 162.3 ms | 163.1 ms | 161.7 ms | 163.1 ms | 706.4 µs | 0.4% |
| git | `fetch_incremental` | 3 | 49.2 ms | 50.4 ms | 48.6 ms | 50.4 ms | 910.2 µs | 1.8% |
| git | `gc` | 3 | 6.35 s | 6.39 s | 6.30 s | 6.39 s | 42.4 ms | 0.7% |
| git | `list_head` | 3 | 2.3 ms | 2.3 ms | 2.2 ms | 2.3 ms | 58.9 µs | 2.6% |
| git | `read_all_versions` | 3 | 1.84 s | 1.84 s | 1.80 s | 1.84 s | 22.0 ms | 1.2% |
| git | `checkout_version` | 3 | 91.0 ms | 93.3 ms | 89.9 ms | 93.3 ms | 1.7 ms | 1.9% |
| git | `ref_delete` | 3 | 1.5 ms | 1.5 ms | 1.4 ms | 1.5 ms | 36.6 µs | 2.5% |
| amber-go | `history_store` | 3 | 8.10 s | 8.13 s | 8.01 s | 8.13 s | 63.2 ms | 0.8% |
| amber-go | `history_store_incremental` | 3 | 2.72 s | 2.72 s | 2.69 s | 2.72 s | 17.8 ms | 0.7% |
| amber-go | `gc` | 3 | 97.7 ms | 100.3 ms | 93.5 ms | 100.3 ms | 3.4 ms | 3.5% |
| amber-go | `list_head` | 3 | 745.3 ms | 769.5 ms | 733.2 ms | 769.5 ms | 18.5 ms | 2.5% |
| amber-go | `read_all_versions` | 3 | 3.67 s | 3.69 s | 3.53 s | 3.69 s | 85.7 ms | 2.4% |
| amber-go | `checkout_version` | 3 | 185.4 ms | 199.3 ms | 184.2 ms | 199.3 ms | 8.4 ms | 4.4% |
| amber-go | `ref_delete` | 3 | 29.4 ms | 30.0 ms | 28.8 ms | 30.0 ms | 612.0 µs | 2.1% |

### What these numbers mean, operation by operation

Read these before comparing two cells above: they state what each measured operation actually delivered.

| backend | operation | note |
|---|---|---|
| amber-go | `history_store` | 39 commands in total; the first few are listed |
| amber-go | `history_store_incremental` | 13 commands in total; the first few are listed |
| amber-go | `list_head` | 29 separate `amber-store ls` processes, one per directory: neither core's CLI has a recursive listing |
| amber-rust | `history_store` | 39 commands in total; the first few are listed |
| amber-rust | `history_store_incremental` | 13 commands in total; the first few are listed |
| amber-rust | `list_head` | 29 separate `amber-store ls` processes, one per directory: neither core's CLI has a recursive listing |
| git | `history_store` | 85 commands in total; the first few are listed |
| git | `history_store_incremental` | 26 commands in total; the first few are listed |

### Recorded counters (median per operation)

| backend | operation | counter | median |
|---|---|---|---|
| amber-rust | `history_store` | `backend_commands` | 39 |
| amber-rust | `history_store` | `commits` | 39 |
| amber-rust | `history_store` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `history_store` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `history_store` | `store_packstore_allocated_bytes` | 110.66 MiB |
| amber-rust | `history_store` | `store_packstore_apparent_bytes` | 110.64 MiB |
| amber-rust | `history_store` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `history_store` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `history_store_incremental` | `backend_commands` | 13 |
| amber-rust | `history_store_incremental` | `commits` | 13 |
| amber-rust | `history_store_incremental` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `history_store_incremental` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `history_store_incremental` | `store_packstore_allocated_bytes` | 111.44 MiB |
| amber-rust | `history_store_incremental` | `store_packstore_apparent_bytes` | 111.43 MiB |
| amber-rust | `history_store_incremental` | `store_refs_allocated_bytes` | 1.57 MiB |
| amber-rust | `history_store_incremental` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `list_head` | `cli_invocations` | 29 |
| amber-rust | `list_head` | `entries` | 1952 |
| git | `history_store` | `backend_commands` | 85 |
| git | `history_store` | `commits` | 39 |
| git | `history_store` | `store_refs_allocated_bytes` | 36.00 KiB |
| git | `history_store` | `store_refs_apparent_bytes` | 205 B |
| git | `history_store_incremental` | `backend_commands` | 26 |
| git | `history_store_incremental` | `commits` | 13 |
| git | `history_store_incremental` | `store_refs_allocated_bytes` | 36.00 KiB |
| git | `history_store_incremental` | `store_refs_apparent_bytes` | 205 B |
| amber-go | `history_store` | `backend_commands` | 39 |
| amber-go | `history_store` | `commits` | 39 |
| amber-go | `history_store` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `history_store` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `history_store` | `store_packstore_allocated_bytes` | 110.60 MiB |
| amber-go | `history_store` | `store_packstore_apparent_bytes` | 110.59 MiB |
| amber-go | `history_store` | `store_refs_allocated_bytes` | 4.57 MiB |
| amber-go | `history_store` | `store_refs_apparent_bytes` | 30.67 KiB |
| amber-go | `history_store_incremental` | `backend_commands` | 13 |
| amber-go | `history_store_incremental` | `commits` | 13 |
| amber-go | `history_store_incremental` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `history_store_incremental` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `history_store_incremental` | `store_packstore_allocated_bytes` | 111.38 MiB |
| amber-go | `history_store_incremental` | `store_packstore_apparent_bytes` | 111.37 MiB |
| amber-go | `history_store_incremental` | `store_refs_allocated_bytes` | 4.62 MiB |
| amber-go | `history_store_incremental` | `store_refs_apparent_bytes` | 40.19 KiB |
| amber-go | `list_head` | `cli_invocations` | 29 |
| amber-go | `list_head` | `entries` | 1952 |

## Scenario `tree/lifecycle`

### Elapsed time (median, lower is better)

| operation | git | amber-rust | amber-go | fs-sha256 | restic |
|---|---|---|---|---|---|
| `ingest_fresh` | 13.83 s | 581.8 ms | 1.39 s | 2.74 s | 1.73 s |
| `ingest_repeat` | 3.15 s | 210.4 ms | 753.4 ms | 1.03 s | 819.3 ms |
| `ingest_changed` | 3.19 s | 208.6 ms | 802.0 ms | 1.14 s | 1.01 s |
| `list_recursive` | 56.4 ms | 5.57 s | 5.04 s | 3.5 ms | 760.9 ms |
| `read_full` | 3.96 s | 452.2 ms | 780.7 ms | 146.3 ms | 2.05 s |
| `restore_full` | 3.94 s | 2.14 s | 1.73 s | 606.5 ms | 1.80 s |
| `ref_delete` | 2.2 ms | 71.5 ms | 66.5 ms | 1.6 ms | 715.4 ms |
| `gc` | 1.7 min | 356.7 ms | 431.7 ms | 22.3 ms | 5.58 s |

### Throughput over the logical bytes processed (median)

| operation | git | amber-rust | amber-go | fs-sha256 | restic |
|---|---|---|---|---|---|
| `ingest_fresh` | 150.24 MiB/s | 3.49 GiB/s | 1.47 GiB/s | 758.53 MiB/s | 1.17 GiB/s |
| `ingest_repeat` | 660.83 MiB/s | 9.65 GiB/s | 2.69 GiB/s | 1.96 GiB/s | 2.48 GiB/s |
| `ingest_changed` | 651.95 MiB/s | 9.73 GiB/s | 2.53 GiB/s | 1.78 GiB/s | 2.01 GiB/s |
| `list_recursive` | — | — | — | — | — |
| `read_full` | 524.27 MiB/s | 4.49 GiB/s | 2.60 GiB/s | 13.88 GiB/s | 1013.74 MiB/s |
| `restore_full` | 527.11 MiB/s | 970.56 MiB/s | 1.18 GiB/s | 3.35 GiB/s | 1.13 GiB/s |
| `ref_delete` | — | — | — | — | — |
| `gc` | — | — | — | — | — |

### CPU time, user + system (median)

| operation | git | amber-rust | amber-go | fs-sha256 | restic |
|---|---|---|---|---|---|
| `ingest_fresh` | 13.77 s | 7.12 s | 27.70 s | 2.17 s | 23.24 s |
| `ingest_repeat` | 3.13 s | 3.20 s | 11.33 s | 1.03 s | 647.9 ms |
| `ingest_changed` | 3.17 s | 3.07 s | 15.67 s | 1.07 s | 5.85 s |
| `list_recursive` | 56.0 ms | 5.47 s | 5.35 s | 3.4 ms | 575.9 ms |
| `read_full` | 3.94 s | 449.4 ms | 1.17 s | 145.5 ms | 3.43 s |
| `restore_full` | 3.92 s | 2.13 s | 2.30 s | 603.3 ms | 2.56 s |
| `ref_delete` | 1.9 ms | 70.0 ms | 70.1 ms | 1.4 ms | 523.4 ms |
| `gc` | 2.9 min | 1.01 s | 1.65 s | 22.1 ms | 6.20 s |

### Peak resident set size (median)

| operation | git | amber-rust | amber-go | fs-sha256 | restic |
|---|---|---|---|---|---|
| `ingest_fresh` | 86.94 MiB | 180.23 MiB | 1.05 GiB | 85.45 MiB | 954.39 MiB |
| `ingest_repeat` | 86.94 MiB | 85.70 MiB | 307.10 MiB | 85.45 MiB | 95.09 MiB |
| `ingest_changed` | 86.94 MiB | 85.70 MiB | 439.57 MiB | 85.45 MiB | 426.46 MiB |
| `list_recursive` | 86.94 MiB | 86.20 MiB | 85.50 MiB | 85.45 MiB | 85.70 MiB |
| `read_full` | 137.18 MiB | 649.05 MiB | 672.56 MiB | 85.45 MiB | 236.94 MiB |
| `restore_full` | 86.94 MiB | 649.57 MiB | 671.24 MiB | 85.45 MiB | 256.35 MiB |
| `ref_delete` | 86.94 MiB | 86.20 MiB | 82.70 MiB | 85.70 MiB | 85.70 MiB |
| `gc` | 2.61 GiB | 617.08 MiB | 679.30 MiB | 85.70 MiB | 323.56 MiB |

### Storage allocated after each operation (median)

| operation | git | amber-rust | amber-go | fs-sha256 | restic |
|---|---|---|---|---|---|
| `ingest_fresh` | 1.28 GiB | 691.08 MiB | 694.76 MiB | 1.54 GiB | 704.88 MiB |
| `ingest_repeat` | 1.28 GiB | 691.09 MiB | 694.77 MiB | 1.54 GiB | 704.89 MiB |
| `ingest_changed` | 1.29 GiB | 696.76 MiB | 700.45 MiB | 1.55 GiB | 710.82 MiB |
| `list_recursive` | — | — | — | — | — |
| `read_full` | — | — | — | — | — |
| `restore_full` | — | — | — | — | — |
| `ref_delete` | 1.29 GiB | 696.76 MiB | 700.45 MiB | 1.55 GiB | 710.81 MiB |
| `gc` | 689.08 MiB | 691.45 MiB | 691.75 MiB | 1.54 GiB | 705.27 MiB |

### Space reclaimed (median; positive means released)

| operation | git | amber-rust | amber-go | fs-sha256 | restic |
|---|---|---|---|---|---|
| `ingest_fresh` | — | — | — | — | — |
| `ingest_repeat` | — | — | — | — | — |
| `ingest_changed` | — | — | — | — | — |
| `list_recursive` | — | — | — | — | — |
| `read_full` | — | — | — | — | — |
| `restore_full` | — | — | — | — | — |
| `ref_delete` | — | — | — | — | — |
| `gc` | 630.32 MiB | 5.32 MiB | 8.70 MiB | 6.67 MiB | 5.54 MiB |

### Dispersion of elapsed time

| backend | operation | n | median | p95 | min | max | stddev | cv |
|---|---|---|---|---|---|---|---|---|
| git | `ingest_fresh` | 3 | 13.83 s | 13.84 s | 13.83 s | 13.84 s | 5.2 ms | 0.0% |
| git | `ingest_repeat` | 3 | 3.15 s | 3.15 s | 3.14 s | 3.15 s | 3.2 ms | 0.1% |
| git | `ingest_changed` | 3 | 3.19 s | 3.19 s | 3.19 s | 3.19 s | 3.2 ms | 0.1% |
| git | `list_recursive` | 3 | 56.4 ms | 56.4 ms | 55.3 ms | 56.4 ms | 613.4 µs | 1.1% |
| git | `read_full` | 3 | 3.96 s | 3.97 s | 3.95 s | 3.97 s | 8.8 ms | 0.2% |
| git | `restore_full` | 3 | 3.94 s | 3.95 s | 3.94 s | 3.95 s | 5.0 ms | 0.1% |
| git | `ref_delete` | 3 | 2.2 ms | 2.4 ms | 2.1 ms | 2.4 ms | 151.3 µs | 6.8% |
| git | `gc` | 3 | 1.7 min | 1.7 min | 1.7 min | 1.7 min | 104.3 ms | 0.1% |
| amber-rust | `ingest_fresh` | 3 | 581.8 ms | 623.7 ms | 573.8 ms | 623.7 ms | 26.8 ms | 4.5% |
| amber-rust | `ingest_repeat` | 3 | 210.4 ms | 213.2 ms | 202.1 ms | 213.2 ms | 5.8 ms | 2.8% |
| amber-rust | `ingest_changed` | 3 | 208.6 ms | 212.2 ms | 202.4 ms | 212.2 ms | 5.0 ms | 2.4% |
| amber-rust | `list_recursive` | 3 | 5.57 s | 5.67 s | 5.45 s | 5.67 s | 108.6 ms | 2.0% |
| amber-rust | `read_full` | 3 | 452.2 ms | 452.9 ms | 448.9 ms | 452.9 ms | 2.1 ms | 0.5% |
| amber-rust | `restore_full` | 3 | 2.14 s | 2.15 s | 2.14 s | 2.15 s | 8.9 ms | 0.4% |
| amber-rust | `ref_delete` | 3 | 71.5 ms | 71.8 ms | 70.2 ms | 71.8 ms | 843.3 µs | 1.2% |
| amber-rust | `gc` | 3 | 356.7 ms | 394.9 ms | 355.4 ms | 394.9 ms | 22.5 ms | 6.1% |
| amber-go | `ingest_fresh` | 3 | 1.39 s | 1.41 s | 1.37 s | 1.41 s | 15.7 ms | 1.1% |
| amber-go | `ingest_repeat` | 3 | 753.4 ms | 756.1 ms | 665.8 ms | 756.1 ms | 51.4 ms | 7.1% |
| amber-go | `ingest_changed` | 3 | 802.0 ms | 837.1 ms | 761.6 ms | 837.1 ms | 37.8 ms | 4.7% |
| amber-go | `list_recursive` | 3 | 5.04 s | 5.07 s | 4.96 s | 5.07 s | 57.2 ms | 1.1% |
| amber-go | `read_full` | 3 | 780.7 ms | 784.2 ms | 780.0 ms | 784.2 ms | 2.2 ms | 0.3% |
| amber-go | `restore_full` | 3 | 1.73 s | 1.74 s | 1.72 s | 1.74 s | 10.9 ms | 0.6% |
| amber-go | `ref_delete` | 3 | 66.5 ms | 66.7 ms | 59.3 ms | 66.7 ms | 4.2 ms | 6.6% |
| amber-go | `gc` | 3 | 431.7 ms | 443.6 ms | 431.7 ms | 443.6 ms | 6.9 ms | 1.6% |
| fs-sha256 | `ingest_fresh` | 3 | 2.74 s | 2.78 s | 2.73 s | 2.78 s | 25.8 ms | 0.9% |
| fs-sha256 | `ingest_repeat` | 3 | 1.03 s | 1.03 s | 1.03 s | 1.03 s | 1.5 ms | 0.1% |
| fs-sha256 | `ingest_changed` | 3 | 1.14 s | 1.14 s | 1.14 s | 1.14 s | 1.8 ms | 0.2% |
| fs-sha256 | `list_recursive` | 3 | 3.5 ms | 3.7 ms | 3.5 ms | 3.7 ms | 85.8 µs | 2.4% |
| fs-sha256 | `read_full` | 3 | 146.3 ms | 146.6 ms | 146.3 ms | 146.6 ms | 202.5 µs | 0.1% |
| fs-sha256 | `restore_full` | 3 | 606.5 ms | 606.6 ms | 605.7 ms | 606.6 ms | 510.4 µs | 0.1% |
| fs-sha256 | `ref_delete` | 3 | 1.6 ms | 1.7 ms | 1.4 ms | 1.7 ms | 145.8 µs | 9.2% |
| fs-sha256 | `gc` | 3 | 22.3 ms | 23.1 ms | 21.9 ms | 23.1 ms | 620.8 µs | 2.8% |
| restic | `ingest_fresh` | 3 | 1.73 s | 1.76 s | 1.73 s | 1.76 s | 20.4 ms | 1.2% |
| restic | `ingest_repeat` | 3 | 819.3 ms | 823.4 ms | 815.6 ms | 823.4 ms | 3.9 ms | 0.5% |
| restic | `ingest_changed` | 3 | 1.01 s | 1.01 s | 1.00 s | 1.01 s | 4.0 ms | 0.4% |
| restic | `list_recursive` | 3 | 760.9 ms | 765.0 ms | 757.6 ms | 765.0 ms | 3.7 ms | 0.5% |
| restic | `read_full` | 3 | 2.05 s | 2.07 s | 2.03 s | 2.07 s | 19.5 ms | 0.9% |
| restic | `restore_full` | 3 | 1.80 s | 1.81 s | 1.78 s | 1.81 s | 14.0 ms | 0.8% |
| restic | `ref_delete` | 3 | 715.4 ms | 729.4 ms | 714.2 ms | 729.4 ms | 8.4 ms | 1.2% |
| restic | `gc` | 3 | 5.58 s | 5.64 s | 5.31 s | 5.64 s | 173.9 ms | 3.2% |

### What these numbers mean, operation by operation

Read these before comparing two cells above: they state what each measured operation actually delivered.

| backend | operation | note |
|---|---|---|
| amber-go | `gc` | reclaimed_packstore_bytes is the pack-segment bytes the collection released; the whole-store reclaim figure also includes the reference database, which compacts itself and is not comparable between the two cores |
| amber-go | `ingest_repeat` | the store already holds every byte; what is measured is how cheaply the backend recognises that |
| amber-go | `list_recursive` | neither Amber CLI has a recursive listing, so this is 167 separate `amber-store ls --keys` processes, one per directory; the number therefore includes 167 process startups and is not comparable with a single-process listing without allowing for them |
| amber-rust | `gc` | reclaimed_packstore_bytes is the pack-segment bytes the collection released; the whole-store reclaim figure also includes the reference database, which compacts itself and is not comparable between the two cores |
| amber-rust | `ingest_repeat` | the store already holds every byte; what is measured is how cheaply the backend recognises that |
| amber-rust | `list_recursive` | neither Amber CLI has a recursive listing, so this is 167 separate `amber-store ls --keys` processes, one per directory; the number therefore includes 167 process startups and is not comparable with a single-process listing without allowing for them |
| fs-sha256 | `ingest_repeat` | the store already holds every byte; what is measured is how cheaply the backend recognises that |
| git | `ingest_repeat` | the store already holds every byte; what is measured is how cheaply the backend recognises that |
| restic | `ingest_repeat` | the store already holds every byte; what is measured is how cheaply the backend recognises that |

### Recorded counters (median per operation)

| backend | operation | counter | median |
|---|---|---|---|
| git | `ingest_fresh` | `store_refs_allocated_bytes` | 16.00 KiB |
| git | `ingest_fresh` | `store_refs_apparent_bytes` | 41 B |
| git | `ingest_repeat` | `store_refs_allocated_bytes` | 20.00 KiB |
| git | `ingest_repeat` | `store_refs_apparent_bytes` | 82 B |
| git | `ingest_changed` | `store_refs_allocated_bytes` | 24.00 KiB |
| git | `ingest_changed` | `store_refs_apparent_bytes` | 123 B |
| git | `ref_delete` | `store_refs_allocated_bytes` | 16.00 KiB |
| git | `ref_delete` | `store_refs_apparent_bytes` | 41 B |
| git | `gc` | `allocated_after_gc` | 722554880 |
| git | `gc` | `allocated_before_delete` | 1383505920 |
| git | `gc` | `allocated_before_gc` | 1383489536 |
| git | `gc` | `store_refs_allocated_bytes` | 12.00 KiB |
| git | `gc` | `store_refs_apparent_bytes` | 0 B |
| amber-rust | `ingest_fresh` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `ingest_fresh` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `ingest_fresh` | `store_packstore_allocated_bytes` | 689.52 MiB |
| amber-rust | `ingest_fresh` | `store_packstore_apparent_bytes` | 689.45 MiB |
| amber-rust | `ingest_fresh` | `store_refs_allocated_bytes` | 1.55 MiB |
| amber-rust | `ingest_fresh` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `ingest_repeat` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `ingest_repeat` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `ingest_repeat` | `store_packstore_allocated_bytes` | 689.52 MiB |
| amber-rust | `ingest_repeat` | `store_packstore_apparent_bytes` | 689.45 MiB |
| amber-rust | `ingest_repeat` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `ingest_repeat` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `ingest_changed` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `ingest_changed` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `ingest_changed` | `store_packstore_allocated_bytes` | 695.20 MiB |
| amber-rust | `ingest_changed` | `store_packstore_apparent_bytes` | 695.13 MiB |
| amber-rust | `ingest_changed` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `ingest_changed` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `list_recursive` | `cli_invocations` | 167 |
| amber-rust | `list_recursive` | `directories` | 166 |
| amber-rust | `list_recursive` | `entries` | 5842 |
| amber-rust | `ref_delete` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `ref_delete` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `ref_delete` | `store_packstore_allocated_bytes` | 695.20 MiB |
| amber-rust | `ref_delete` | `store_packstore_apparent_bytes` | 695.13 MiB |
| amber-rust | `ref_delete` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `ref_delete` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `gc` | `allocated_after_gc` | 725032960 |
| amber-rust | `gc` | `allocated_before_delete` | 730607616 |
| amber-rust | `gc` | `allocated_before_gc` | 730607616 |
| amber-rust | `gc` | `reclaimed_packstore_bytes` | 5.32 MiB |
| amber-rust | `gc` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `gc` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `gc` | `store_packstore_allocated_bytes` | 689.88 MiB |
| amber-rust | `gc` | `store_packstore_apparent_bytes` | 689.82 MiB |
| amber-rust | `gc` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `gc` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-go | `ingest_fresh` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `ingest_fresh` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `ingest_fresh` | `store_packstore_allocated_bytes` | 690.34 MiB |
| amber-go | `ingest_fresh` | `store_packstore_apparent_bytes` | 690.27 MiB |
| amber-go | `ingest_fresh` | `store_refs_allocated_bytes` | 4.41 MiB |
| amber-go | `ingest_fresh` | `store_refs_apparent_bytes` | 2.85 KiB |
| amber-go | `ingest_repeat` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `ingest_repeat` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `ingest_repeat` | `store_packstore_allocated_bytes` | 690.34 MiB |
| amber-go | `ingest_repeat` | `store_packstore_apparent_bytes` | 690.27 MiB |
| amber-go | `ingest_repeat` | `store_refs_allocated_bytes` | 4.42 MiB |
| amber-go | `ingest_repeat` | `store_refs_apparent_bytes` | 3.55 KiB |
| amber-go | `ingest_changed` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `ingest_changed` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `ingest_changed` | `store_packstore_allocated_bytes` | 696.02 MiB |
| amber-go | `ingest_changed` | `store_packstore_apparent_bytes` | 695.95 MiB |
| amber-go | `ingest_changed` | `store_refs_allocated_bytes` | 4.43 MiB |
| amber-go | `ingest_changed` | `store_refs_apparent_bytes` | 4.26 KiB |
| amber-go | `list_recursive` | `cli_invocations` | 167 |
| amber-go | `list_recursive` | `directories` | 166 |
| amber-go | `list_recursive` | `entries` | 5842 |
| amber-go | `ref_delete` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `ref_delete` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `ref_delete` | `store_packstore_allocated_bytes` | 696.02 MiB |
| amber-go | `ref_delete` | `store_packstore_apparent_bytes` | 695.95 MiB |
| amber-go | `ref_delete` | `store_refs_allocated_bytes` | 4.42 MiB |
| amber-go | `ref_delete` | `store_refs_apparent_bytes` | 3.93 KiB |
| amber-go | `gc` | `allocated_after_gc` | 725348352 |
| amber-go | `gc` | `allocated_before_delete` | 729866240 |
| amber-go | `gc` | `allocated_before_gc` | 734470144 |
| amber-go | `gc` | `reclaimed_packstore_bytes` | 4.30 MiB |
| amber-go | `gc` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `gc` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `gc` | `store_packstore_allocated_bytes` | 691.71 MiB |
| amber-go | `gc` | `store_packstore_apparent_bytes` | 691.64 MiB |
| amber-go | `gc` | `store_refs_allocated_bytes` | 28.00 KiB |
| amber-go | `gc` | `store_refs_apparent_bytes` | 4.46 KiB |
| fs-sha256 | `ingest_fresh` | `store_refs_allocated_bytes` | 932.00 KiB |
| fs-sha256 | `ingest_fresh` | `store_refs_apparent_bytes` | 927.38 KiB |
| fs-sha256 | `ingest_repeat` | `store_refs_allocated_bytes` | 1.82 MiB |
| fs-sha256 | `ingest_repeat` | `store_refs_apparent_bytes` | 1.81 MiB |
| fs-sha256 | `ingest_changed` | `store_refs_allocated_bytes` | 2.72 MiB |
| fs-sha256 | `ingest_changed` | `store_refs_apparent_bytes` | 2.72 MiB |
| fs-sha256 | `ref_delete` | `store_refs_allocated_bytes` | 932.00 KiB |
| fs-sha256 | `ref_delete` | `store_refs_apparent_bytes` | 927.69 KiB |
| fs-sha256 | `gc` | `allocated_after_gc` | 1654947840 |
| fs-sha256 | `gc` | `allocated_before_delete` | 1663840256 |
| fs-sha256 | `gc` | `allocated_before_gc` | 1661939712 |
| fs-sha256 | `gc` | `store_refs_allocated_bytes` | 932.00 KiB |
| fs-sha256 | `gc` | `store_refs_apparent_bytes` | 927.69 KiB |
| restic | `ingest_fresh` | `store_data_allocated_bytes` | 704.56 MiB |
| restic | `ingest_fresh` | `store_data_apparent_bytes` | 703.48 MiB |
| restic | `ingest_fresh` | `store_index_allocated_bytes` | 300.00 KiB |
| restic | `ingest_fresh` | `store_index_apparent_bytes` | 295.59 KiB |
| restic | `ingest_fresh` | `store_snapshots_allocated_bytes` | 8.00 KiB |
| restic | `ingest_fresh` | `store_snapshots_apparent_bytes` | 415 B |
| restic | `ingest_repeat` | `store_data_allocated_bytes` | 704.56 MiB |
| restic | `ingest_repeat` | `store_data_apparent_bytes` | 703.48 MiB |
| restic | `ingest_repeat` | `store_index_allocated_bytes` | 300.00 KiB |
| restic | `ingest_repeat` | `store_index_apparent_bytes` | 295.59 KiB |
| restic | `ingest_repeat` | `store_snapshots_allocated_bytes` | 12.00 KiB |
| restic | `ingest_repeat` | `store_snapshots_apparent_bytes` | 867 B |
| restic | `ingest_changed` | `store_data_allocated_bytes` | 710.44 MiB |
| restic | `ingest_changed` | `store_data_apparent_bytes` | 709.35 MiB |
| restic | `ingest_changed` | `store_index_allocated_bytes` | 352.00 KiB |
| restic | `ingest_changed` | `store_index_apparent_bytes` | 344.30 KiB |
| restic | `ingest_changed` | `store_snapshots_allocated_bytes` | 16.00 KiB |
| restic | `ingest_changed` | `store_snapshots_apparent_bytes` | 1.25 KiB |
| restic | `ref_delete` | `store_data_allocated_bytes` | 710.44 MiB |
| restic | `ref_delete` | `store_data_apparent_bytes` | 709.35 MiB |
| restic | `ref_delete` | `store_index_allocated_bytes` | 352.00 KiB |
| restic | `ref_delete` | `store_index_apparent_bytes` | 344.30 KiB |
| restic | `ref_delete` | `store_snapshots_allocated_bytes` | 8.00 KiB |
| restic | `ref_delete` | `store_snapshots_apparent_bytes` | 411 B |
| restic | `gc` | `allocated_after_gc` | 739532800 |
| restic | `gc` | `allocated_before_delete` | 745345024 |
| restic | `gc` | `allocated_before_gc` | 745336832 |
| restic | `gc` | `store_data_allocated_bytes` | 704.95 MiB |
| restic | `gc` | `store_data_apparent_bytes` | 703.86 MiB |
| restic | `gc` | `store_index_allocated_bytes` | 300.00 KiB |
| restic | `gc` | `store_index_apparent_bytes` | 295.74 KiB |
| restic | `gc` | `store_snapshots_allocated_bytes` | 8.00 KiB |
| restic | `gc` | `store_snapshots_apparent_bytes` | 411 B |

## Scenario `nix/closure`

### Elapsed time (median, lower is better)

| operation | nix | amber-rust | amber-go |
|---|---|---|---|
| `import_initial` | 82.3 ms | 218.3 ms | 332.6 ms |
| `import_repeat` | 20.8 ms | 293.0 ms | 305.7 ms |
| `import_incremental` | 73.3 ms | 401.4 ms | 437.0 ms |
| `catalog_list` | 15.0 ms | 34.5 ms | 30.1 ms |
| `closure_query` | 14.7 ms | unsupported | unsupported |
| `read_closure` | 149.1 ms | 317.5 ms | 364.9 ms |
| `cache_export` | 19.34 s | unsupported | unsupported |
| `materialize_closure` | 74.3 ms | 469.2 ms | 431.5 ms |
| `cache_substitute` | 108.2 ms | unsupported | unsupported |
| `delete_path` | 23.2 ms | 34.0 ms | 31.6 ms |
| `gc` | 23.0 ms | 34.3 ms | 31.2 ms |
| `nar_and_reference_registration` | — | unsupported | unsupported |

### Throughput over the logical bytes processed (median)

| operation | nix | amber-rust | amber-go |
|---|---|---|---|
| `import_initial` | 2.09 GiB/s | 806.36 MiB/s | 529.10 MiB/s |
| `import_repeat` | 8.27 GiB/s | 600.71 MiB/s | 575.64 MiB/s |
| `import_incremental` | 2.45 GiB/s | 458.45 MiB/s | 421.09 MiB/s |
| `catalog_list` | — | — | — |
| `closure_query` | — | unsupported | unsupported |
| `read_closure` | 1.21 GiB/s | 579.58 MiB/s | 504.23 MiB/s |
| `cache_export` | — | unsupported | unsupported |
| `materialize_closure` | 2.42 GiB/s | 392.17 MiB/s | 426.41 MiB/s |
| `cache_substitute` | — | unsupported | unsupported |
| `delete_path` | — | — | — |
| `gc` | — | — | — |
| `nar_and_reference_registration` | — | unsupported | unsupported |

### CPU time, user + system (median)

| operation | nix | amber-rust | amber-go |
|---|---|---|---|
| `import_initial` | 154.0 ms | 521.2 ms | 1.67 s |
| `import_repeat` | 17.7 ms | 446.5 ms | 487.0 ms |
| `import_incremental` | 76.5 ms | 593.3 ms | 990.7 ms |
| `catalog_list` | 15.7 ms | 33.9 ms | 32.1 ms |
| `closure_query` | 15.4 ms | unsupported | unsupported |
| `read_closure` | 154.8 ms | 311.9 ms | 402.1 ms |
| `cache_export` | 30.35 s | unsupported | unsupported |
| `materialize_closure` | 156.8 ms | 462.2 ms | 502.3 ms |
| `cache_substitute` | 240.8 ms | unsupported | unsupported |
| `delete_path` | 23.4 ms | 33.4 ms | 33.6 ms |
| `gc` | 23.5 ms | 33.5 ms | 33.8 ms |
| `nar_and_reference_registration` | — | unsupported | unsupported |

### Peak resident set size (median)

| operation | nix | amber-rust | amber-go |
|---|---|---|---|
| `import_initial` | 82.20 MiB | 83.95 MiB | 250.58 MiB |
| `import_repeat` | 82.20 MiB | 83.95 MiB | 99.77 MiB |
| `import_incremental` | 82.20 MiB | 84.20 MiB | 151.08 MiB |
| `catalog_list` | 82.20 MiB | 84.20 MiB | 85.20 MiB |
| `closure_query` | 82.20 MiB | unsupported | unsupported |
| `read_closure` | 82.20 MiB | 84.20 MiB | 173.91 MiB |
| `cache_export` | 477.66 MiB | unsupported | unsupported |
| `materialize_closure` | 82.20 MiB | 84.20 MiB | 172.61 MiB |
| `cache_substitute` | 126.03 MiB | unsupported | unsupported |
| `delete_path` | 82.20 MiB | 84.20 MiB | 85.45 MiB |
| `gc` | 82.20 MiB | 84.20 MiB | 85.45 MiB |
| `nar_and_reference_registration` | — | unsupported | unsupported |

### Storage allocated after each operation (median)

| operation | nix | amber-rust | amber-go |
|---|---|---|---|
| `import_initial` | 184.29 MiB | 105.74 MiB | 108.62 MiB |
| `import_repeat` | 184.29 MiB | 105.74 MiB | 108.62 MiB |
| `import_incremental` | 264.33 MiB | 121.76 MiB | 124.64 MiB |
| `catalog_list` | — | — | — |
| `closure_query` | — | unsupported | unsupported |
| `read_closure` | — | — | — |
| `cache_export` | — | unsupported | unsupported |
| `materialize_closure` | — | — | — |
| `cache_substitute` | — | unsupported | unsupported |
| `delete_path` | 248.32 MiB | 121.76 MiB | 124.65 MiB |
| `gc` | 184.31 MiB | 121.80 MiB | 120.27 MiB |
| `nar_and_reference_registration` | — | unsupported | unsupported |

### Space reclaimed (median; positive means released)

| operation | nix | amber-rust | amber-go |
|---|---|---|---|
| `import_initial` | — | — | — |
| `import_repeat` | — | — | — |
| `import_incremental` | — | — | — |
| `catalog_list` | — | — | — |
| `closure_query` | — | unsupported | unsupported |
| `read_closure` | — | — | — |
| `cache_export` | — | unsupported | unsupported |
| `materialize_closure` | — | — | — |
| `cache_substitute` | — | unsupported | unsupported |
| `delete_path` | — | — | — |
| `gc` | 64.01 MiB | -40.00 KiB | 4.38 MiB |
| `nar_and_reference_registration` | — | unsupported | unsupported |

### Dispersion of elapsed time

| backend | operation | n | median | p95 | min | max | stddev | cv |
|---|---|---|---|---|---|---|---|---|
| nix | `import_initial` | 3 | 82.3 ms | 94.8 ms | 81.3 ms | 94.8 ms | 7.5 ms | 8.7% |
| nix | `import_repeat` | 3 | 20.8 ms | 22.3 ms | 20.4 ms | 22.3 ms | 1.0 ms | 4.8% |
| nix | `import_incremental` | 3 | 73.3 ms | 73.8 ms | 72.2 ms | 73.8 ms | 857.1 µs | 1.2% |
| nix | `catalog_list` | 3 | 15.0 ms | 16.9 ms | 14.5 ms | 16.9 ms | 1.3 ms | 8.3% |
| nix | `closure_query` | 3 | 14.7 ms | 15.0 ms | 14.5 ms | 15.0 ms | 235.2 µs | 1.6% |
| nix | `read_closure` | 3 | 149.1 ms | 149.7 ms | 148.2 ms | 149.7 ms | 762.5 µs | 0.5% |
| nix | `cache_export` | 3 | 19.34 s | 19.39 s | 19.12 s | 19.39 s | 141.6 ms | 0.7% |
| nix | `materialize_closure` | 3 | 74.3 ms | 74.8 ms | 74.0 ms | 74.8 ms | 423.1 µs | 0.6% |
| nix | `cache_substitute` | 3 | 108.2 ms | 112.2 ms | 107.6 ms | 112.2 ms | 2.5 ms | 2.3% |
| nix | `delete_path` | 3 | 23.2 ms | 23.3 ms | 21.7 ms | 23.3 ms | 905.9 µs | 4.0% |
| nix | `gc` | 3 | 23.0 ms | 24.4 ms | 21.9 ms | 24.4 ms | 1.3 ms | 5.6% |
| amber-rust | `import_initial` | 3 | 218.3 ms | 220.8 ms | 218.0 ms | 220.8 ms | 1.6 ms | 0.7% |
| amber-rust | `import_repeat` | 3 | 293.0 ms | 295.4 ms | 289.9 ms | 295.4 ms | 2.7 ms | 0.9% |
| amber-rust | `import_incremental` | 3 | 401.4 ms | 406.4 ms | 398.4 ms | 406.4 ms | 4.0 ms | 1.0% |
| amber-rust | `catalog_list` | 3 | 34.5 ms | 34.5 ms | 34.2 ms | 34.5 ms | 208.1 µs | 0.6% |
| amber-rust | `read_closure` | 3 | 317.5 ms | 322.0 ms | 306.6 ms | 322.0 ms | 7.9 ms | 2.5% |
| amber-rust | `materialize_closure` | 3 | 469.2 ms | 478.5 ms | 460.2 ms | 478.5 ms | 9.2 ms | 2.0% |
| amber-rust | `delete_path` | 3 | 34.0 ms | 34.2 ms | 32.5 ms | 34.2 ms | 932.6 µs | 2.8% |
| amber-rust | `gc` | 3 | 34.3 ms | 34.9 ms | 34.0 ms | 34.9 ms | 501.2 µs | 1.5% |
| amber-go | `import_initial` | 3 | 332.6 ms | 335.8 ms | 331.4 ms | 335.8 ms | 2.3 ms | 0.7% |
| amber-go | `import_repeat` | 3 | 305.7 ms | 306.5 ms | 299.3 ms | 306.5 ms | 3.9 ms | 1.3% |
| amber-go | `import_incremental` | 3 | 437.0 ms | 443.7 ms | 434.7 ms | 443.7 ms | 4.7 ms | 1.1% |
| amber-go | `catalog_list` | 3 | 30.1 ms | 31.0 ms | 30.0 ms | 31.0 ms | 528.5 µs | 1.7% |
| amber-go | `read_closure` | 3 | 364.9 ms | 384.1 ms | 364.0 ms | 384.1 ms | 11.4 ms | 3.1% |
| amber-go | `materialize_closure` | 3 | 431.5 ms | 458.0 ms | 425.5 ms | 458.0 ms | 17.3 ms | 3.9% |
| amber-go | `delete_path` | 3 | 31.6 ms | 32.5 ms | 28.3 ms | 32.5 ms | 2.2 ms | 7.1% |
| amber-go | `gc` | 3 | 31.2 ms | 32.6 ms | 29.7 ms | 32.6 ms | 1.5 ms | 4.7% |

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
| nix | `gc` | `allocated_before_delete` | 277172224 |
| amber-rust | `import_initial` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `import_initial` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `import_initial` | `store_packstore_allocated_bytes` | 104.17 MiB |
| amber-rust | `import_initial` | `store_packstore_apparent_bytes` | 104.16 MiB |
| amber-rust | `import_initial` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `import_initial` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `import_repeat` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `import_repeat` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `import_repeat` | `store_packstore_allocated_bytes` | 104.17 MiB |
| amber-rust | `import_repeat` | `store_packstore_apparent_bytes` | 104.16 MiB |
| amber-rust | `import_repeat` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `import_repeat` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `import_incremental` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `import_incremental` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `import_incremental` | `store_packstore_allocated_bytes` | 120.20 MiB |
| amber-rust | `import_incremental` | `store_packstore_apparent_bytes` | 120.18 MiB |
| amber-rust | `import_incremental` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `import_incremental` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `delete_path` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `delete_path` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `delete_path` | `store_packstore_allocated_bytes` | 120.20 MiB |
| amber-rust | `delete_path` | `store_packstore_apparent_bytes` | 120.18 MiB |
| amber-rust | `delete_path` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `delete_path` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `gc` | `allocated_before_delete` | 127676416 |
| amber-rust | `gc` | `reclaimed_packstore_bytes` | -40.00 KiB |
| amber-rust | `gc` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `gc` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `gc` | `store_packstore_allocated_bytes` | 120.23 MiB |
| amber-rust | `gc` | `store_packstore_apparent_bytes` | 120.22 MiB |
| amber-rust | `gc` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `gc` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-go | `import_initial` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `import_initial` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `import_initial` | `store_packstore_allocated_bytes` | 104.17 MiB |
| amber-go | `import_initial` | `store_packstore_apparent_bytes` | 104.16 MiB |
| amber-go | `import_initial` | `store_refs_allocated_bytes` | 4.45 MiB |
| amber-go | `import_initial` | `store_refs_apparent_bytes` | 10.01 KiB |
| amber-go | `import_repeat` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `import_repeat` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `import_repeat` | `store_packstore_allocated_bytes` | 104.17 MiB |
| amber-go | `import_repeat` | `store_packstore_apparent_bytes` | 104.16 MiB |
| amber-go | `import_repeat` | `store_refs_allocated_bytes` | 4.45 MiB |
| amber-go | `import_repeat` | `store_refs_apparent_bytes` | 10.67 KiB |
| amber-go | `import_incremental` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `import_incremental` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `import_incremental` | `store_packstore_allocated_bytes` | 120.20 MiB |
| amber-go | `import_incremental` | `store_packstore_apparent_bytes` | 120.18 MiB |
| amber-go | `import_incremental` | `store_refs_allocated_bytes` | 4.44 MiB |
| amber-go | `import_incremental` | `store_refs_apparent_bytes` | 9.85 KiB |
| amber-go | `delete_path` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `delete_path` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `delete_path` | `store_packstore_allocated_bytes` | 120.20 MiB |
| amber-go | `delete_path` | `store_packstore_apparent_bytes` | 120.18 MiB |
| amber-go | `delete_path` | `store_refs_allocated_bytes` | 4.45 MiB |
| amber-go | `delete_path` | `store_refs_apparent_bytes` | 10.92 KiB |
| amber-go | `gc` | `allocated_before_delete` | 126091264 |
| amber-go | `gc` | `reclaimed_packstore_bytes` | -40.00 KiB |
| amber-go | `gc` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `gc` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `gc` | `store_packstore_allocated_bytes` | 120.23 MiB |
| amber-go | `gc` | `store_packstore_apparent_bytes` | 120.22 MiB |
| amber-go | `gc` | `store_refs_allocated_bytes` | 24.00 KiB |
| amber-go | `gc` | `store_refs_apparent_bytes` | 7.14 KiB |

## Scenario `blob/source-history`

### Elapsed time (median, lower is better)

| operation | amber-rust-s3 | git-bundle-s3 | amber-go-s3 |
|---|---|---|---|
| `push_initial` | 192.0 ms | 6.56 s | 196.1 ms |
| `push_noop` | 18.7 ms | 18.7 ms | 19.2 ms |
| `pull_fresh` | 67.8 ms | 664.9 ms | 75.1 ms |
| `push_incremental` | 172.7 ms | 59.2 ms | 170.7 ms |
| `pull_incremental` | 63.5 ms | 79.1 ms | 103.1 ms |
| `retention_cleanup` | 190.7 ms | 6.61 s | 210.2 ms |

### Throughput over the logical bytes processed (median)

| operation | amber-rust-s3 | git-bundle-s3 | amber-go-s3 |
|---|---|---|---|
| `push_initial` | 595.59 MiB/s | 16.59 MiB/s | 565.31 MiB/s |
| `push_noop` | 156.60 KiB/s | 123.23 KiB/s | 820.21 KiB/s |
| `pull_fresh` | 1.64 GiB/s | 163.54 MiB/s | 1.44 GiB/s |
| `push_incremental` | 294.50 MiB/s | 3.99 MiB/s | 277.67 MiB/s |
| `pull_incremental` | 798.67 MiB/s | 3.04 MiB/s | 459.10 MiB/s |
| `retention_cleanup` | 558.43 MiB/s | 16.50 MiB/s | 490.99 MiB/s |

### CPU time, user + system (median)

| operation | amber-rust-s3 | git-bundle-s3 | amber-go-s3 |
|---|---|---|---|
| `push_initial` | 153.9 ms | 6.66 s | 198.2 ms |
| `push_noop` | 25.4 ms | 25.1 ms | 27.7 ms |
| `pull_fresh` | 74.6 ms | 662.0 ms | 117.7 ms |
| `push_incremental` | 75.2 ms | 83.3 ms | 98.5 ms |
| `pull_incremental` | 55.2 ms | 42.2 ms | 76.2 ms |
| `retention_cleanup` | 142.3 ms | 6.77 s | 219.2 ms |

### Peak resident set size (median)

| operation | amber-rust-s3 | git-bundle-s3 | amber-go-s3 |
|---|---|---|---|
| `push_initial` | 83.45 MiB | 247.07 MiB | 81.45 MiB |
| `push_noop` | 83.45 MiB | 83.45 MiB | 81.45 MiB |
| `pull_fresh` | 83.45 MiB | 113.96 MiB | 81.45 MiB |
| `push_incremental` | 83.45 MiB | 83.45 MiB | 81.45 MiB |
| `pull_incremental` | 83.45 MiB | 83.45 MiB | 81.45 MiB |
| `retention_cleanup` | 83.45 MiB | 247.67 MiB | 81.95 MiB |

### Dispersion of elapsed time

| backend | operation | n | median | p95 | min | max | stddev | cv |
|---|---|---|---|---|---|---|---|---|
| amber-rust-s3 | `push_initial` | 3 | 192.0 ms | 192.6 ms | 189.6 ms | 192.6 ms | 1.6 ms | 0.8% |
| amber-rust-s3 | `push_noop` | 3 | 18.7 ms | 19.1 ms | 18.3 ms | 19.1 ms | 389.1 µs | 2.1% |
| amber-rust-s3 | `pull_fresh` | 3 | 67.8 ms | 75.8 ms | 67.5 ms | 75.8 ms | 4.7 ms | 6.7% |
| amber-rust-s3 | `push_incremental` | 3 | 172.7 ms | 173.4 ms | 164.4 ms | 173.4 ms | 5.0 ms | 3.0% |
| amber-rust-s3 | `pull_incremental` | 3 | 63.5 ms | 96.7 ms | 53.6 ms | 96.7 ms | 22.6 ms | 31.7% |
| amber-rust-s3 | `retention_cleanup` | 3 | 190.7 ms | 194.6 ms | 184.1 ms | 194.6 ms | 5.3 ms | 2.8% |
| git-bundle-s3 | `push_initial` | 3 | 6.56 s | 6.59 s | 6.56 s | 6.59 s | 14.0 ms | 0.2% |
| git-bundle-s3 | `push_noop` | 3 | 18.7 ms | 19.5 ms | 18.3 ms | 19.5 ms | 580.4 µs | 3.1% |
| git-bundle-s3 | `pull_fresh` | 3 | 664.9 ms | 680.9 ms | 660.1 ms | 680.9 ms | 10.9 ms | 1.6% |
| git-bundle-s3 | `push_incremental` | 3 | 59.2 ms | 59.2 ms | 59.1 ms | 59.2 ms | 53.5 µs | 0.1% |
| git-bundle-s3 | `pull_incremental` | 3 | 79.1 ms | 80.0 ms | 37.3 ms | 80.0 ms | 24.4 ms | 37.3% |
| git-bundle-s3 | `retention_cleanup` | 3 | 6.61 s | 6.65 s | 6.59 s | 6.65 s | 31.0 ms | 0.5% |
| amber-go-s3 | `push_initial` | 3 | 196.1 ms | 198.5 ms | 194.2 ms | 198.5 ms | 2.1 ms | 1.1% |
| amber-go-s3 | `push_noop` | 3 | 19.2 ms | 19.7 ms | 19.1 ms | 19.7 ms | 317.4 µs | 1.6% |
| amber-go-s3 | `pull_fresh` | 3 | 75.1 ms | 79.0 ms | 69.8 ms | 79.0 ms | 4.6 ms | 6.2% |
| amber-go-s3 | `push_incremental` | 3 | 170.7 ms | 173.5 ms | 168.0 ms | 173.5 ms | 2.7 ms | 1.6% |
| amber-go-s3 | `pull_incremental` | 3 | 103.1 ms | 108.8 ms | 64.7 ms | 108.8 ms | 24.0 ms | 26.0% |
| amber-go-s3 | `retention_cleanup` | 3 | 210.2 ms | 216.7 ms | 205.9 ms | 216.7 ms | 5.4 ms | 2.6% |

### What these numbers mean, operation by operation

Read these before comparing two cells above: they state what each measured operation actually delivered.

| backend | operation | note |
|---|---|---|
| amber-go-s3 | `pull_fresh` | delivered to the destination: the whole published store, which holds all 39 reference(s) published so far; these cores have no selective fetch |
| amber-go-s3 | `pull_fresh` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `pull_incremental` | delivered to the destination: the store's new pack segments and reference database, bringing the client up to the 13 reference(s) the incremental publication added |
| amber-go-s3 | `pull_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `push_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `push_initial` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `push_noop` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `retention_cleanup` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-go-s3 | `retention_cleanup` | retained by this retention step: the 13 reference(s) of the incremental publication; the 39 of the initial one were dropped and collected. Retention is the one operation in this scenario where the backends are not asked for the same thing — consolidating Git bundles still publishes the whole history, while dropping an Amber reference, forgetting a restic snapshot or expiring a narinfo removes a version — so these timings are not a like-for-like ranking. What each one kept is stated here, and every member of it is verified afterwards. |
| amber-rust-s3 | `pull_fresh` | delivered to the destination: the whole published store, which holds all 39 reference(s) published so far; these cores have no selective fetch |
| amber-rust-s3 | `pull_fresh` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `pull_incremental` | delivered to the destination: the store's new pack segments and reference database, bringing the client up to the 13 reference(s) the incremental publication added |
| amber-rust-s3 | `pull_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `push_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `push_initial` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `push_noop` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `retention_cleanup` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| amber-rust-s3 | `retention_cleanup` | retained by this retention step: the 13 reference(s) of the incremental publication; the 39 of the initial one were dropped and collected. Retention is the one operation in this scenario where the backends are not asked for the same thing — consolidating Git bundles still publishes the whole history, while dropping an Amber reference, forgetting a restic snapshot or expiring a narinfo removes a version — so these timings are not a like-for-like ranking. What each one kept is stated here, and every member of it is verified afterwards. |
| git-bundle-s3 | `pull_fresh` | delivered to the destination: the whole published history: 39 commits, every branch and tag of them |
| git-bundle-s3 | `pull_fresh` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| git-bundle-s3 | `pull_incremental` | delivered to the destination: the 13 commits added to main since the first publication, into a clone that already held the rest |
| git-bundle-s3 | `pull_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| git-bundle-s3 | `push_incremental` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| git-bundle-s3 | `push_initial` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| git-bundle-s3 | `push_noop` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| git-bundle-s3 | `retention_cleanup` | for a transfer, the logical byte count is the bytes that actually crossed the wire in both directions, so the throughput column is wire throughput rather than payload throughput |
| git-bundle-s3 | `retention_cleanup` | retained by this retention step: the whole history: 52 commits with every branch and tag, in one consolidated bundle. Consolidating bundles drops no version — what was superseded is the two bundle objects, not any commit. Retention is the one operation in this scenario where the backends are not asked for the same thing — consolidating Git bundles still publishes the whole history, while dropping an Amber reference, forgetting a restic snapshot or expiring a narinfo removes a version — so these timings are not a like-for-like ranking. What each one kept is stated here, and every member of it is verified afterwards. |

### Recorded counters (median per operation)

| backend | operation | counter | median |
|---|---|---|---|
| amber-rust-s3 | `push_initial` | `s3_bytes_down` | 4.04 KiB |
| amber-rust-s3 | `push_initial` | `s3_bytes_up` | 114.33 MiB |
| amber-rust-s3 | `push_initial` | `s3_connections` | 8 |
| amber-rust-s3 | `push_initial` | `s3_request_body_bytes` | 114.32 MiB |
| amber-rust-s3 | `push_initial` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_initial` | `s3_requests_POST` | 4 |
| amber-rust-s3 | `push_initial` | `s3_requests_PUT` | 9 |
| amber-rust-s3 | `push_initial` | `s3_requests_total` | 15 |
| amber-rust-s3 | `push_initial` | `s3_response_body_bytes` | 2.41 KiB |
| amber-rust-s3 | `push_initial` | `s3_responses_2xx` | 14 |
| amber-rust-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_initial` | `stored_bytes` | 114.16 MiB |
| amber-rust-s3 | `push_initial` | `stored_objects` | 3 |
| amber-rust-s3 | `push_noop` | `s3_bytes_down` | 1.76 KiB |
| amber-rust-s3 | `push_noop` | `s3_bytes_up` | 1.17 KiB |
| amber-rust-s3 | `push_noop` | `s3_connections` | 1 |
| amber-rust-s3 | `push_noop` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `push_noop` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_noop` | `s3_requests_total` | 2 |
| amber-rust-s3 | `push_noop` | `s3_response_body_bytes` | 1.54 KiB |
| amber-rust-s3 | `push_noop` | `s3_responses_2xx` | 1 |
| amber-rust-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_noop` | `stored_bytes` | 114.16 MiB |
| amber-rust-s3 | `push_noop` | `stored_objects` | 3 |
| amber-rust-s3 | `pull_fresh` | `delivered_logical_bytes` | 4.02 GiB |
| amber-rust-s3 | `pull_fresh` | `delivered_references` | 39 |
| amber-rust-s3 | `pull_fresh` | `s3_bytes_down` | 114.16 MiB |
| amber-rust-s3 | `pull_fresh` | `s3_bytes_up` | 5.06 KiB |
| amber-rust-s3 | `pull_fresh` | `s3_connections` | 3 |
| amber-rust-s3 | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `pull_fresh` | `s3_requests_GET` | 7 |
| amber-rust-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| amber-rust-s3 | `pull_fresh` | `s3_requests_total` | 8 |
| amber-rust-s3 | `pull_fresh` | `s3_response_body_bytes` | 114.16 MiB |
| amber-rust-s3 | `pull_fresh` | `s3_responses_2xx` | 6 |
| amber-rust-s3 | `pull_fresh` | `s3_responses_4xx` | 2 |
| amber-rust-s3 | `pull_fresh` | `stored_bytes` | 114.16 MiB |
| amber-rust-s3 | `pull_fresh` | `stored_objects` | 3 |
| amber-rust-s3 | `push_incremental` | `s3_bytes_down` | 3.36 KiB |
| amber-rust-s3 | `push_incremental` | `s3_bytes_up` | 50.85 MiB |
| amber-rust-s3 | `push_incremental` | `s3_connections` | 4 |
| amber-rust-s3 | `push_incremental` | `s3_request_body_bytes` | 50.84 MiB |
| amber-rust-s3 | `push_incremental` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_requests_POST` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_requests_PUT` | 4 |
| amber-rust-s3 | `push_incremental` | `s3_requests_total` | 8 |
| amber-rust-s3 | `push_incremental` | `s3_response_body_bytes` | 2.45 KiB |
| amber-rust-s3 | `push_incremental` | `s3_responses_2xx` | 7 |
| amber-rust-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_incremental` | `stored_bytes` | 114.94 MiB |
| amber-rust-s3 | `push_incremental` | `stored_objects` | 3 |
| amber-rust-s3 | `pull_incremental` | `delivered_logical_bytes` | 1.34 GiB |
| amber-rust-s3 | `pull_incremental` | `delivered_references` | 13 |
| amber-rust-s3 | `pull_incremental` | `s3_bytes_down` | 50.78 MiB |
| amber-rust-s3 | `pull_incremental` | `s3_bytes_up` | 4.45 KiB |
| amber-rust-s3 | `pull_incremental` | `s3_connections` | 2 |
| amber-rust-s3 | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `pull_incremental` | `s3_requests_GET` | 6 |
| amber-rust-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| amber-rust-s3 | `pull_incremental` | `s3_requests_total` | 7 |
| amber-rust-s3 | `pull_incremental` | `s3_response_body_bytes` | 50.78 MiB |
| amber-rust-s3 | `pull_incremental` | `s3_responses_2xx` | 5 |
| amber-rust-s3 | `pull_incremental` | `s3_responses_4xx` | 2 |
| amber-rust-s3 | `pull_incremental` | `stored_bytes` | 114.94 MiB |
| amber-rust-s3 | `pull_incremental` | `stored_objects` | 3 |
| amber-rust-s3 | `retention_cleanup` | `retained_logical_bytes` | 1.34 GiB |
| amber-rust-s3 | `retention_cleanup` | `retained_references` | 13 |
| amber-rust-s3 | `retention_cleanup` | `s3_bytes_down` | 5.95 KiB |
| amber-rust-s3 | `retention_cleanup` | `s3_bytes_up` | 106.47 MiB |
| amber-rust-s3 | `retention_cleanup` | `s3_connections` | 8 |
| amber-rust-s3 | `retention_cleanup` | `s3_request_body_bytes` | 106.45 MiB |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_POST` | 6 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_PUT` | 8 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_total` | 16 |
| amber-rust-s3 | `retention_cleanup` | `s3_response_body_bytes` | 4.22 KiB |
| amber-rust-s3 | `retention_cleanup` | `s3_responses_2xx` | 15 |
| amber-rust-s3 | `retention_cleanup` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `retention_cleanup` | `stored_bytes` | 106.31 MiB |
| amber-rust-s3 | `retention_cleanup` | `stored_objects` | 3 |
| git-bundle-s3 | `push_initial` | `s3_bytes_down` | 2.62 KiB |
| git-bundle-s3 | `push_initial` | `s3_bytes_up` | 108.89 MiB |
| git-bundle-s3 | `push_initial` | `s3_connections` | 4 |
| git-bundle-s3 | `push_initial` | `s3_request_body_bytes` | 108.89 MiB |
| git-bundle-s3 | `push_initial` | `s3_requests_GET` | 2 |
| git-bundle-s3 | `push_initial` | `s3_requests_POST` | 2 |
| git-bundle-s3 | `push_initial` | `s3_requests_PUT` | 7 |
| git-bundle-s3 | `push_initial` | `s3_requests_total` | 11 |
| git-bundle-s3 | `push_initial` | `s3_response_body_bytes` | 1.45 KiB |
| git-bundle-s3 | `push_initial` | `s3_responses_2xx` | 10 |
| git-bundle-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| git-bundle-s3 | `push_initial` | `stored_bytes` | 108.74 MiB |
| git-bundle-s3 | `push_initial` | `stored_objects` | 1 |
| git-bundle-s3 | `push_noop` | `s3_bytes_down` | 1.12 KiB |
| git-bundle-s3 | `push_noop` | `s3_bytes_up` | 1.17 KiB |
| git-bundle-s3 | `push_noop` | `s3_connections` | 1 |
| git-bundle-s3 | `push_noop` | `s3_request_body_bytes` | 0 B |
| git-bundle-s3 | `push_noop` | `s3_requests_GET` | 2 |
| git-bundle-s3 | `push_noop` | `s3_requests_total` | 2 |
| git-bundle-s3 | `push_noop` | `s3_response_body_bytes` | 927 B |
| git-bundle-s3 | `push_noop` | `s3_responses_2xx` | 1 |
| git-bundle-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| git-bundle-s3 | `push_noop` | `stored_bytes` | 108.74 MiB |
| git-bundle-s3 | `push_noop` | `stored_objects` | 1 |
| git-bundle-s3 | `pull_fresh` | `delivered_logical_bytes` | 4.02 GiB |
| git-bundle-s3 | `pull_fresh` | `delivered_references` | 39 |
| git-bundle-s3 | `pull_fresh` | `s3_bytes_down` | 108.74 MiB |
| git-bundle-s3 | `pull_fresh` | `s3_bytes_up` | 3.04 KiB |
| git-bundle-s3 | `pull_fresh` | `s3_connections` | 1 |
| git-bundle-s3 | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| git-bundle-s3 | `pull_fresh` | `s3_requests_GET` | 4 |
| git-bundle-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| git-bundle-s3 | `pull_fresh` | `s3_requests_total` | 5 |
| git-bundle-s3 | `pull_fresh` | `s3_response_body_bytes` | 108.74 MiB |
| git-bundle-s3 | `pull_fresh` | `s3_responses_2xx` | 3 |
| git-bundle-s3 | `pull_fresh` | `s3_responses_4xx` | 2 |
| git-bundle-s3 | `pull_fresh` | `stored_bytes` | 108.74 MiB |
| git-bundle-s3 | `pull_fresh` | `stored_objects` | 1 |
| git-bundle-s3 | `push_incremental` | `s3_bytes_down` | 1.32 KiB |
| git-bundle-s3 | `push_incremental` | `s3_bytes_up` | 240.88 KiB |
| git-bundle-s3 | `push_incremental` | `s3_connections` | 1 |
| git-bundle-s3 | `push_incremental` | `s3_request_body_bytes` | 239.01 KiB |
| git-bundle-s3 | `push_incremental` | `s3_requests_GET` | 2 |
| git-bundle-s3 | `push_incremental` | `s3_requests_PUT` | 1 |
| git-bundle-s3 | `push_incremental` | `s3_requests_total` | 3 |
| git-bundle-s3 | `push_incremental` | `s3_response_body_bytes` | 927 B |
| git-bundle-s3 | `push_incremental` | `s3_responses_2xx` | 2 |
| git-bundle-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| git-bundle-s3 | `push_incremental` | `stored_bytes` | 108.97 MiB |
| git-bundle-s3 | `push_incremental` | `stored_objects` | 2 |
| git-bundle-s3 | `pull_incremental` | `delivered_logical_bytes` | 1.34 GiB |
| git-bundle-s3 | `pull_incremental` | `delivered_references` | 13 |
| git-bundle-s3 | `pull_incremental` | `s3_bytes_down` | 242.22 KiB |
| git-bundle-s3 | `pull_incremental` | `s3_bytes_up` | 3.86 KiB |
| git-bundle-s3 | `pull_incremental` | `s3_connections` | 1 |
| git-bundle-s3 | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| git-bundle-s3 | `pull_incremental` | `s3_requests_GET` | 5 |
| git-bundle-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| git-bundle-s3 | `pull_incremental` | `s3_requests_total` | 6 |
| git-bundle-s3 | `pull_incremental` | `s3_response_body_bytes` | 241.46 KiB |
| git-bundle-s3 | `pull_incremental` | `s3_responses_2xx` | 4 |
| git-bundle-s3 | `pull_incremental` | `s3_responses_4xx` | 2 |
| git-bundle-s3 | `pull_incremental` | `stored_bytes` | 108.97 MiB |
| git-bundle-s3 | `pull_incremental` | `stored_objects` | 2 |
| git-bundle-s3 | `retention_cleanup` | `retained_logical_bytes` | 5.36 GiB |
| git-bundle-s3 | `retention_cleanup` | `retained_references` | 52 |
| git-bundle-s3 | `retention_cleanup` | `s3_bytes_down` | 5.33 KiB |
| git-bundle-s3 | `retention_cleanup` | `s3_bytes_up` | 109.13 MiB |
| git-bundle-s3 | `retention_cleanup` | `s3_connections` | 6 |
| git-bundle-s3 | `retention_cleanup` | `s3_request_body_bytes` | 109.12 MiB |
| git-bundle-s3 | `retention_cleanup` | `s3_requests_GET` | 4 |
| git-bundle-s3 | `retention_cleanup` | `s3_requests_HEAD` | 4 |
| git-bundle-s3 | `retention_cleanup` | `s3_requests_POST` | 4 |
| git-bundle-s3 | `retention_cleanup` | `s3_requests_PUT` | 7 |
| git-bundle-s3 | `retention_cleanup` | `s3_requests_total` | 19 |
| git-bundle-s3 | `retention_cleanup` | `s3_response_body_bytes` | 2.81 KiB |
| git-bundle-s3 | `retention_cleanup` | `s3_responses_2xx` | 16 |
| git-bundle-s3 | `retention_cleanup` | `s3_responses_4xx` | 3 |
| git-bundle-s3 | `retention_cleanup` | `stored_bytes` | 108.97 MiB |
| git-bundle-s3 | `retention_cleanup` | `stored_objects` | 1 |
| amber-go-s3 | `push_initial` | `s3_bytes_down` | 12.67 KiB |
| amber-go-s3 | `push_initial` | `s3_bytes_up` | 110.87 MiB |
| amber-go-s3 | `push_initial` | `s3_connections` | 13 |
| amber-go-s3 | `push_initial` | `s3_request_body_bytes` | 110.83 MiB |
| amber-go-s3 | `push_initial` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_initial` | `s3_requests_POST` | 4 |
| amber-go-s3 | `push_initial` | `s3_requests_PUT` | 53 |
| amber-go-s3 | `push_initial` | `s3_requests_total` | 59 |
| amber-go-s3 | `push_initial` | `s3_response_body_bytes` | 2.40 KiB |
| amber-go-s3 | `push_initial` | `s3_responses_2xx` | 58 |
| amber-go-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_initial` | `stored_bytes` | 110.67 MiB |
| amber-go-s3 | `push_initial` | `stored_objects` | 47 |
| amber-go-s3 | `push_noop` | `s3_bytes_down` | 14.56 KiB |
| amber-go-s3 | `push_noop` | `s3_bytes_up` | 1.17 KiB |
| amber-go-s3 | `push_noop` | `s3_connections` | 1 |
| amber-go-s3 | `push_noop` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `push_noop` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_noop` | `s3_requests_total` | 2 |
| amber-go-s3 | `push_noop` | `s3_response_body_bytes` | 14.34 KiB |
| amber-go-s3 | `push_noop` | `s3_responses_2xx` | 1 |
| amber-go-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_noop` | `stored_bytes` | 110.67 MiB |
| amber-go-s3 | `push_noop` | `stored_objects` | 47 |
| amber-go-s3 | `pull_fresh` | `delivered_logical_bytes` | 4.02 GiB |
| amber-go-s3 | `pull_fresh` | `delivered_references` | 39 |
| amber-go-s3 | `pull_fresh` | `s3_bytes_down` | 110.69 MiB |
| amber-go-s3 | `pull_fresh` | `s3_bytes_up` | 31.05 KiB |
| amber-go-s3 | `pull_fresh` | `s3_connections` | 14 |
| amber-go-s3 | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `pull_fresh` | `s3_requests_GET` | 51 |
| amber-go-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| amber-go-s3 | `pull_fresh` | `s3_requests_total` | 52 |
| amber-go-s3 | `pull_fresh` | `s3_response_body_bytes` | 110.68 MiB |
| amber-go-s3 | `pull_fresh` | `s3_responses_2xx` | 50 |
| amber-go-s3 | `pull_fresh` | `s3_responses_4xx` | 2 |
| amber-go-s3 | `pull_fresh` | `stored_bytes` | 110.67 MiB |
| amber-go-s3 | `pull_fresh` | `stored_objects` | 47 |
| amber-go-s3 | `push_incremental` | `s3_bytes_down` | 22.37 KiB |
| amber-go-s3 | `push_incremental` | `s3_bytes_up` | 47.37 MiB |
| amber-go-s3 | `push_incremental` | `s3_connections` | 12 |
| amber-go-s3 | `push_incremental` | `s3_request_body_bytes` | 47.35 MiB |
| amber-go-s3 | `push_incremental` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_incremental` | `s3_requests_POST` | 7 |
| amber-go-s3 | `push_incremental` | `s3_requests_PUT` | 22 |
| amber-go-s3 | `push_incremental` | `s3_requests_total` | 31 |
| amber-go-s3 | `push_incremental` | `s3_response_body_bytes` | 17.39 KiB |
| amber-go-s3 | `push_incremental` | `s3_responses_2xx` | 30 |
| amber-go-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_incremental` | `stored_bytes` | 111.46 MiB |
| amber-go-s3 | `push_incremental` | `stored_objects` | 60 |
| amber-go-s3 | `pull_incremental` | `delivered_logical_bytes` | 1.34 GiB |
| amber-go-s3 | `pull_incremental` | `delivered_references` | 13 |
| amber-go-s3 | `pull_incremental` | `s3_bytes_down` | 47.31 MiB |
| amber-go-s3 | `pull_incremental` | `s3_bytes_up` | 15.09 KiB |
| amber-go-s3 | `pull_incremental` | `s3_connections` | 10 |
| amber-go-s3 | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `pull_incremental` | `s3_requests_GET` | 24 |
| amber-go-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| amber-go-s3 | `pull_incremental` | `s3_requests_total` | 25 |
| amber-go-s3 | `pull_incremental` | `s3_response_body_bytes` | 47.30 MiB |
| amber-go-s3 | `pull_incremental` | `s3_responses_2xx` | 23 |
| amber-go-s3 | `pull_incremental` | `s3_responses_4xx` | 2 |
| amber-go-s3 | `pull_incremental` | `stored_bytes` | 111.46 MiB |
| amber-go-s3 | `pull_incremental` | `stored_objects` | 60 |
| amber-go-s3 | `retention_cleanup` | `retained_logical_bytes` | 1.34 GiB |
| amber-go-s3 | `retention_cleanup` | `retained_references` | 13 |
| amber-go-s3 | `retention_cleanup` | `s3_bytes_down` | 60.67 KiB |
| amber-go-s3 | `retention_cleanup` | `s3_bytes_up` | 103.05 MiB |
| amber-go-s3 | `retention_cleanup` | `s3_connections` | 37 |
| amber-go-s3 | `retention_cleanup` | `s3_request_body_bytes` | 102.98 MiB |
| amber-go-s3 | `retention_cleanup` | `s3_requests_GET` | 2 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_POST` | 62 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_PUT` | 52 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_total` | 116 |
| amber-go-s3 | `retention_cleanup` | `s3_response_body_bytes` | 44.40 KiB |
| amber-go-s3 | `retention_cleanup` | `s3_responses_2xx` | 115 |
| amber-go-s3 | `retention_cleanup` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `retention_cleanup` | `stored_bytes` | 102.82 MiB |
| amber-go-s3 | `retention_cleanup` | `stored_objects` | 48 |

## Scenario `blob/backup-corpus`

### Elapsed time (median, lower is better)

| operation | restic-s3 | amber-go-s3 | amber-rust-s3 |
|---|---|---|---|
| `push_initial` | 3.01 s | 685.9 ms | 681.4 ms |
| `push_noop` | 828.3 ms | 21.1 ms | 20.7 ms |
| `pull_fresh` | 1.59 s | 256.9 ms | 261.7 ms |
| `push_incremental` | 1.08 s | 175.2 ms | 173.0 ms |
| `pull_incremental` | 1.59 s | 72.3 ms | 70.6 ms |
| `retention_cleanup` | 3.50 s | 688.5 ms | 577.0 ms |

### Throughput over the logical bytes processed (median)

| operation | restic-s3 | amber-go-s3 | amber-rust-s3 |
|---|---|---|---|
| `push_initial` | 236.54 MiB/s | 1007.97 MiB/s | 1018.46 MiB/s |
| `push_noop` | 17.14 KiB/s | 340.34 KiB/s | 276.67 KiB/s |
| `pull_fresh` | 446.37 MiB/s | 2.62 GiB/s | 2.59 GiB/s |
| `push_incremental` | 5.51 MiB/s | 314.96 MiB/s | 332.71 MiB/s |
| `pull_incremental` | 454.48 MiB/s | 761.62 MiB/s | 813.67 MiB/s |
| `retention_cleanup` | 312.02 MiB/s | 1006.06 MiB/s | 1.04 GiB/s |

### CPU time, user + system (median)

| operation | restic-s3 | amber-go-s3 | amber-rust-s3 |
|---|---|---|---|
| `push_initial` | 24.04 s | 1.21 s | 1.24 s |
| `push_noop` | 651.3 ms | 25.8 ms | 27.1 ms |
| `pull_fresh` | 2.98 s | 866.4 ms | 847.6 ms |
| `push_incremental` | 5.90 s | 86.2 ms | 86.3 ms |
| `pull_incremental` | 3.01 s | 62.0 ms | 59.2 ms |
| `retention_cleanup` | 8.71 s | 1.25 s | 989.4 ms |

### Peak resident set size (median)

| operation | restic-s3 | amber-go-s3 | amber-rust-s3 |
|---|---|---|---|
| `push_initial` | 1.04 GiB | 83.45 MiB | 86.45 MiB |
| `push_noop` | 95.40 MiB | 83.70 MiB | 86.45 MiB |
| `pull_fresh` | 369.64 MiB | 83.70 MiB | 86.45 MiB |
| `push_incremental` | 437.97 MiB | 83.70 MiB | 86.69 MiB |
| `pull_incremental` | 368.61 MiB | 83.70 MiB | 86.69 MiB |
| `retention_cleanup` | 551.15 MiB | 83.95 MiB | 86.69 MiB |

### Dispersion of elapsed time

| backend | operation | n | median | p95 | min | max | stddev | cv |
|---|---|---|---|---|---|---|---|---|
| restic-s3 | `push_initial` | 3 | 3.01 s | 3.02 s | 2.96 s | 3.02 s | 31.1 ms | 1.0% |
| restic-s3 | `push_noop` | 3 | 828.3 ms | 831.9 ms | 824.7 ms | 831.9 ms | 3.6 ms | 0.4% |
| restic-s3 | `pull_fresh` | 3 | 1.59 s | 1.61 s | 1.58 s | 1.61 s | 13.2 ms | 0.8% |
| restic-s3 | `push_incremental` | 3 | 1.08 s | 1.08 s | 1.07 s | 1.08 s | 5.4 ms | 0.5% |
| restic-s3 | `pull_incremental` | 3 | 1.59 s | 1.60 s | 1.56 s | 1.60 s | 24.0 ms | 1.5% |
| restic-s3 | `retention_cleanup` | 3 | 3.50 s | 3.79 s | 3.40 s | 3.79 s | 203.1 ms | 5.7% |
| amber-go-s3 | `push_initial` | 3 | 685.9 ms | 687.0 ms | 675.7 ms | 687.0 ms | 6.2 ms | 0.9% |
| amber-go-s3 | `push_noop` | 3 | 21.1 ms | 21.8 ms | 20.0 ms | 21.8 ms | 943.5 µs | 4.5% |
| amber-go-s3 | `pull_fresh` | 3 | 256.9 ms | 285.6 ms | 256.5 ms | 285.6 ms | 16.7 ms | 6.3% |
| amber-go-s3 | `push_incremental` | 3 | 175.2 ms | 180.1 ms | 173.0 ms | 180.1 ms | 3.6 ms | 2.1% |
| amber-go-s3 | `pull_incremental` | 3 | 72.3 ms | 109.4 ms | 66.5 ms | 109.4 ms | 23.3 ms | 28.1% |
| amber-go-s3 | `retention_cleanup` | 3 | 688.5 ms | 697.3 ms | 635.1 ms | 697.3 ms | 33.7 ms | 5.0% |
| amber-rust-s3 | `push_initial` | 3 | 681.4 ms | 691.9 ms | 673.1 ms | 691.9 ms | 9.4 ms | 1.4% |
| amber-rust-s3 | `push_noop` | 3 | 20.7 ms | 21.1 ms | 20.0 ms | 21.1 ms | 553.3 µs | 2.7% |
| amber-rust-s3 | `pull_fresh` | 3 | 261.7 ms | 287.4 ms | 254.2 ms | 287.4 ms | 17.4 ms | 6.5% |
| amber-rust-s3 | `push_incremental` | 3 | 173.0 ms | 175.7 ms | 172.3 ms | 175.7 ms | 1.8 ms | 1.0% |
| amber-rust-s3 | `pull_incremental` | 3 | 70.6 ms | 76.2 ms | 68.4 ms | 76.2 ms | 4.0 ms | 5.6% |
| amber-rust-s3 | `retention_cleanup` | 3 | 577.0 ms | 591.6 ms | 521.7 ms | 591.6 ms | 36.9 ms | 6.5% |

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
| restic-s3 | `push_initial` | `s3_bytes_down` | 14.03 KiB |
| restic-s3 | `push_initial` | `s3_bytes_up` | 709.10 MiB |
| restic-s3 | `push_initial` | `s3_connections` | 5 |
| restic-s3 | `push_initial` | `s3_request_body_bytes` | 709.06 MiB |
| restic-s3 | `push_initial` | `s3_requests_DELETE` | 1 |
| restic-s3 | `push_initial` | `s3_requests_GET` | 8 |
| restic-s3 | `push_initial` | `s3_requests_HEAD` | 1 |
| restic-s3 | `push_initial` | `s3_requests_PUT` | 47 |
| restic-s3 | `push_initial` | `s3_requests_total` | 57 |
| restic-s3 | `push_initial` | `s3_response_body_bytes` | 3.43 KiB |
| restic-s3 | `push_initial` | `s3_responses_2xx` | 56 |
| restic-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| restic-s3 | `push_initial` | `stored_bytes` | 708.09 MiB |
| restic-s3 | `push_initial` | `stored_objects` | 48 |
| restic-s3 | `push_noop` | `s3_bytes_down` | 5.88 KiB |
| restic-s3 | `push_noop` | `s3_bytes_up` | 8.31 KiB |
| restic-s3 | `push_noop` | `s3_connections` | 1 |
| restic-s3 | `push_noop` | `s3_request_body_bytes` | 970 B |
| restic-s3 | `push_noop` | `s3_requests_DELETE` | 1 |
| restic-s3 | `push_noop` | `s3_requests_GET` | 8 |
| restic-s3 | `push_noop` | `s3_requests_HEAD` | 1 |
| restic-s3 | `push_noop` | `s3_requests_PUT` | 2 |
| restic-s3 | `push_noop` | `s3_requests_total` | 12 |
| restic-s3 | `push_noop` | `s3_response_body_bytes` | 4.12 KiB |
| restic-s3 | `push_noop` | `s3_responses_2xx` | 11 |
| restic-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| restic-s3 | `push_noop` | `stored_bytes` | 708.09 MiB |
| restic-s3 | `push_noop` | `stored_objects` | 49 |
| restic-s3 | `pull_fresh` | `delivered_logical_bytes` | 2.03 GiB |
| restic-s3 | `pull_fresh` | `delivered_references` | 1 |
| restic-s3 | `pull_fresh` | `s3_bytes_down` | 707.85 MiB |
| restic-s3 | `pull_fresh` | `s3_bytes_up` | 34.56 KiB |
| restic-s3 | `pull_fresh` | `s3_connections` | 5 |
| restic-s3 | `pull_fresh` | `s3_request_body_bytes` | 345 B |
| restic-s3 | `pull_fresh` | `s3_requests_DELETE` | 1 |
| restic-s3 | `pull_fresh` | `s3_requests_GET` | 53 |
| restic-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| restic-s3 | `pull_fresh` | `s3_requests_PUT` | 1 |
| restic-s3 | `pull_fresh` | `s3_requests_total` | 56 |
| restic-s3 | `pull_fresh` | `s3_response_body_bytes` | 707.84 MiB |
| restic-s3 | `pull_fresh` | `s3_responses_2xx` | 55 |
| restic-s3 | `pull_fresh` | `s3_responses_4xx` | 1 |
| restic-s3 | `pull_fresh` | `stored_bytes` | 708.09 MiB |
| restic-s3 | `pull_fresh` | `stored_objects` | 49 |
| restic-s3 | `push_incremental` | `s3_bytes_down` | 6.81 KiB |
| restic-s3 | `push_incremental` | `s3_bytes_up` | 5.93 MiB |
| restic-s3 | `push_incremental` | `s3_connections` | 1 |
| restic-s3 | `push_incremental` | `s3_request_body_bytes` | 5.92 MiB |
| restic-s3 | `push_incremental` | `s3_requests_DELETE` | 1 |
| restic-s3 | `push_incremental` | `s3_requests_GET` | 8 |
| restic-s3 | `push_incremental` | `s3_requests_HEAD` | 1 |
| restic-s3 | `push_incremental` | `s3_requests_PUT` | 5 |
| restic-s3 | `push_incremental` | `s3_requests_total` | 15 |
| restic-s3 | `push_incremental` | `s3_response_body_bytes` | 4.46 KiB |
| restic-s3 | `push_incremental` | `s3_responses_2xx` | 14 |
| restic-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| restic-s3 | `push_incremental` | `stored_bytes` | 714.00 MiB |
| restic-s3 | `push_incremental` | `stored_objects` | 53 |
| restic-s3 | `pull_incremental` | `delivered_logical_bytes` | 2.03 GiB |
| restic-s3 | `pull_incremental` | `delivered_references` | 1 |
| restic-s3 | `pull_incremental` | `s3_bytes_down` | 712.96 MiB |
| restic-s3 | `pull_incremental` | `s3_bytes_up` | 35.18 KiB |
| restic-s3 | `pull_incremental` | `s3_connections` | 5 |
| restic-s3 | `pull_incremental` | `s3_request_body_bytes` | 345 B |
| restic-s3 | `pull_incremental` | `s3_requests_DELETE` | 1 |
| restic-s3 | `pull_incremental` | `s3_requests_GET` | 54 |
| restic-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| restic-s3 | `pull_incremental` | `s3_requests_PUT` | 1 |
| restic-s3 | `pull_incremental` | `s3_requests_total` | 57 |
| restic-s3 | `pull_incremental` | `s3_response_body_bytes` | 712.95 MiB |
| restic-s3 | `pull_incremental` | `s3_responses_2xx` | 56 |
| restic-s3 | `pull_incremental` | `s3_responses_4xx` | 1 |
| restic-s3 | `pull_incremental` | `stored_bytes` | 714.00 MiB |
| restic-s3 | `pull_incremental` | `stored_objects` | 53 |
| restic-s3 | `retention_cleanup` | `retained_logical_bytes` | 2.03 GiB |
| restic-s3 | `retention_cleanup` | `retained_references` | 1 |
| restic-s3 | `retention_cleanup` | `s3_bytes_down` | 548.00 MiB |
| restic-s3 | `retention_cleanup` | `s3_bytes_up` | 544.52 MiB |
| restic-s3 | `retention_cleanup` | `s3_connections` | 7 |
| restic-s3 | `retention_cleanup` | `s3_request_body_bytes` | 544.43 MiB |
| restic-s3 | `retention_cleanup` | `s3_requests_DELETE` | 41 |
| restic-s3 | `retention_cleanup` | `s3_requests_GET` | 49 |
| restic-s3 | `retention_cleanup` | `s3_requests_HEAD` | 2 |
| restic-s3 | `retention_cleanup` | `s3_requests_PUT` | 38 |
| restic-s3 | `retention_cleanup` | `s3_requests_total` | 130 |
| restic-s3 | `retention_cleanup` | `s3_response_body_bytes` | 547.97 MiB |
| restic-s3 | `retention_cleanup` | `s3_responses_2xx` | 128 |
| restic-s3 | `retention_cleanup` | `s3_responses_4xx` | 2 |
| restic-s3 | `retention_cleanup` | `stored_bytes` | 708.46 MiB |
| restic-s3 | `retention_cleanup` | `stored_objects` | 49 |
| amber-go-s3 | `push_initial` | `s3_bytes_down` | 19.57 KiB |
| amber-go-s3 | `push_initial` | `s3_bytes_up` | 691.31 MiB |
| amber-go-s3 | `push_initial` | `s3_connections` | 48 |
| amber-go-s3 | `push_initial` | `s3_request_body_bytes` | 691.25 MiB |
| amber-go-s3 | `push_initial` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_initial` | `s3_requests_POST` | 22 |
| amber-go-s3 | `push_initial` | `s3_requests_PUT` | 60 |
| amber-go-s3 | `push_initial` | `s3_requests_total` | 84 |
| amber-go-s3 | `push_initial` | `s3_response_body_bytes` | 10.35 KiB |
| amber-go-s3 | `push_initial` | `s3_responses_2xx` | 83 |
| amber-go-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_initial` | `stored_bytes` | 690.29 MiB |
| amber-go-s3 | `push_initial` | `stored_objects` | 17 |
| amber-go-s3 | `push_noop` | `s3_bytes_down` | 6.03 KiB |
| amber-go-s3 | `push_noop` | `s3_bytes_up` | 1.17 KiB |
| amber-go-s3 | `push_noop` | `s3_connections` | 1 |
| amber-go-s3 | `push_noop` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `push_noop` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_noop` | `s3_requests_total` | 2 |
| amber-go-s3 | `push_noop` | `s3_response_body_bytes` | 5.81 KiB |
| amber-go-s3 | `push_noop` | `s3_responses_2xx` | 1 |
| amber-go-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_noop` | `stored_bytes` | 690.29 MiB |
| amber-go-s3 | `push_noop` | `stored_objects` | 17 |
| amber-go-s3 | `pull_fresh` | `delivered_logical_bytes` | 2.03 GiB |
| amber-go-s3 | `pull_fresh` | `delivered_references` | 1 |
| amber-go-s3 | `pull_fresh` | `s3_bytes_down` | 690.31 MiB |
| amber-go-s3 | `pull_fresh` | `s3_bytes_up` | 13.46 KiB |
| amber-go-s3 | `pull_fresh` | `s3_connections` | 17 |
| amber-go-s3 | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `pull_fresh` | `s3_requests_GET` | 21 |
| amber-go-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| amber-go-s3 | `pull_fresh` | `s3_requests_total` | 22 |
| amber-go-s3 | `pull_fresh` | `s3_response_body_bytes` | 690.30 MiB |
| amber-go-s3 | `pull_fresh` | `s3_responses_2xx` | 20 |
| amber-go-s3 | `pull_fresh` | `s3_responses_4xx` | 2 |
| amber-go-s3 | `pull_fresh` | `stored_bytes` | 690.29 MiB |
| amber-go-s3 | `pull_fresh` | `stored_objects` | 17 |
| amber-go-s3 | `push_incremental` | `s3_bytes_down` | 10.33 KiB |
| amber-go-s3 | `push_incremental` | `s3_bytes_up` | 54.99 MiB |
| amber-go-s3 | `push_incremental` | `s3_connections` | 9 |
| amber-go-s3 | `push_incremental` | `s3_request_body_bytes` | 54.98 MiB |
| amber-go-s3 | `push_incremental` | `s3_requests_GET` | 2 |
| amber-go-s3 | `push_incremental` | `s3_requests_POST` | 5 |
| amber-go-s3 | `push_incremental` | `s3_requests_PUT` | 10 |
| amber-go-s3 | `push_incremental` | `s3_requests_total` | 17 |
| amber-go-s3 | `push_incremental` | `s3_response_body_bytes` | 8.00 KiB |
| amber-go-s3 | `push_incremental` | `s3_responses_2xx` | 16 |
| amber-go-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `push_incremental` | `stored_bytes` | 695.97 MiB |
| amber-go-s3 | `push_incremental` | `stored_objects` | 19 |
| amber-go-s3 | `pull_incremental` | `delivered_logical_bytes` | 2.03 GiB |
| amber-go-s3 | `pull_incremental` | `delivered_references` | 1 |
| amber-go-s3 | `pull_incremental` | `s3_bytes_down` | 54.91 MiB |
| amber-go-s3 | `pull_incremental` | `s3_bytes_up` | 7.41 KiB |
| amber-go-s3 | `pull_incremental` | `s3_connections` | 7 |
| amber-go-s3 | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| amber-go-s3 | `pull_incremental` | `s3_requests_GET` | 11 |
| amber-go-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| amber-go-s3 | `pull_incremental` | `s3_requests_total` | 12 |
| amber-go-s3 | `pull_incremental` | `s3_response_body_bytes` | 54.91 MiB |
| amber-go-s3 | `pull_incremental` | `s3_responses_2xx` | 10 |
| amber-go-s3 | `pull_incremental` | `s3_responses_4xx` | 2 |
| amber-go-s3 | `pull_incremental` | `stored_bytes` | 695.97 MiB |
| amber-go-s3 | `pull_incremental` | `stored_objects` | 19 |
| amber-go-s3 | `retention_cleanup` | `retained_logical_bytes` | 2.03 GiB |
| amber-go-s3 | `retention_cleanup` | `retained_references` | 1 |
| amber-go-s3 | `retention_cleanup` | `s3_bytes_down` | 34.58 KiB |
| amber-go-s3 | `retention_cleanup` | `s3_bytes_up` | 692.51 MiB |
| amber-go-s3 | `retention_cleanup` | `s3_connections` | 50 |
| amber-go-s3 | `retention_cleanup` | `s3_request_body_bytes` | 692.44 MiB |
| amber-go-s3 | `retention_cleanup` | `s3_requests_GET` | 2 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_POST` | 39 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_PUT` | 60 |
| amber-go-s3 | `retention_cleanup` | `s3_requests_total` | 101 |
| amber-go-s3 | `retention_cleanup` | `s3_response_body_bytes` | 23.48 KiB |
| amber-go-s3 | `retention_cleanup` | `s3_responses_2xx` | 100 |
| amber-go-s3 | `retention_cleanup` | `s3_responses_4xx` | 1 |
| amber-go-s3 | `retention_cleanup` | `stored_bytes` | 691.48 MiB |
| amber-go-s3 | `retention_cleanup` | `stored_objects` | 19 |
| amber-rust-s3 | `push_initial` | `s3_bytes_down` | 18.65 KiB |
| amber-rust-s3 | `push_initial` | `s3_bytes_up` | 693.99 MiB |
| amber-rust-s3 | `push_initial` | `s3_connections` | 45 |
| amber-rust-s3 | `push_initial` | `s3_request_body_bytes` | 693.93 MiB |
| amber-rust-s3 | `push_initial` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_initial` | `s3_requests_POST` | 22 |
| amber-rust-s3 | `push_initial` | `s3_requests_PUT` | 55 |
| amber-rust-s3 | `push_initial` | `s3_requests_total` | 79 |
| amber-rust-s3 | `push_initial` | `s3_response_body_bytes` | 10.41 KiB |
| amber-rust-s3 | `push_initial` | `s3_responses_2xx` | 78 |
| amber-rust-s3 | `push_initial` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_initial` | `stored_bytes` | 692.97 MiB |
| amber-rust-s3 | `push_initial` | `stored_objects` | 12 |
| amber-rust-s3 | `push_noop` | `s3_bytes_down` | 4.57 KiB |
| amber-rust-s3 | `push_noop` | `s3_bytes_up` | 1.17 KiB |
| amber-rust-s3 | `push_noop` | `s3_connections` | 1 |
| amber-rust-s3 | `push_noop` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `push_noop` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_noop` | `s3_requests_total` | 2 |
| amber-rust-s3 | `push_noop` | `s3_response_body_bytes` | 4.35 KiB |
| amber-rust-s3 | `push_noop` | `s3_responses_2xx` | 1 |
| amber-rust-s3 | `push_noop` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_noop` | `stored_bytes` | 692.97 MiB |
| amber-rust-s3 | `push_noop` | `stored_objects` | 12 |
| amber-rust-s3 | `pull_fresh` | `delivered_logical_bytes` | 2.03 GiB |
| amber-rust-s3 | `pull_fresh` | `delivered_references` | 1 |
| amber-rust-s3 | `pull_fresh` | `s3_bytes_down` | 692.98 MiB |
| amber-rust-s3 | `pull_fresh` | `s3_bytes_up` | 10.49 KiB |
| amber-rust-s3 | `pull_fresh` | `s3_connections` | 12 |
| amber-rust-s3 | `pull_fresh` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `pull_fresh` | `s3_requests_GET` | 16 |
| amber-rust-s3 | `pull_fresh` | `s3_requests_HEAD` | 1 |
| amber-rust-s3 | `pull_fresh` | `s3_requests_total` | 17 |
| amber-rust-s3 | `pull_fresh` | `s3_response_body_bytes` | 692.97 MiB |
| amber-rust-s3 | `pull_fresh` | `s3_responses_2xx` | 15 |
| amber-rust-s3 | `pull_fresh` | `s3_responses_4xx` | 2 |
| amber-rust-s3 | `pull_fresh` | `stored_bytes` | 692.97 MiB |
| amber-rust-s3 | `pull_fresh` | `stored_objects` | 12 |
| amber-rust-s3 | `push_incremental` | `s3_bytes_down` | 6.28 KiB |
| amber-rust-s3 | `push_incremental` | `s3_bytes_up` | 57.54 MiB |
| amber-rust-s3 | `push_incremental` | `s3_connections` | 5 |
| amber-rust-s3 | `push_incremental` | `s3_request_body_bytes` | 57.54 MiB |
| amber-rust-s3 | `push_incremental` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_requests_POST` | 2 |
| amber-rust-s3 | `push_incremental` | `s3_requests_PUT` | 5 |
| amber-rust-s3 | `push_incremental` | `s3_requests_total` | 9 |
| amber-rust-s3 | `push_incremental` | `s3_response_body_bytes` | 5.26 KiB |
| amber-rust-s3 | `push_incremental` | `s3_responses_2xx` | 8 |
| amber-rust-s3 | `push_incremental` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `push_incremental` | `stored_bytes` | 698.64 MiB |
| amber-rust-s3 | `push_incremental` | `stored_objects` | 12 |
| amber-rust-s3 | `pull_incremental` | `delivered_logical_bytes` | 2.03 GiB |
| amber-rust-s3 | `pull_incremental` | `delivered_references` | 1 |
| amber-rust-s3 | `pull_incremental` | `s3_bytes_down` | 57.47 MiB |
| amber-rust-s3 | `pull_incremental` | `s3_bytes_up` | 4.44 KiB |
| amber-rust-s3 | `pull_incremental` | `s3_connections` | 2 |
| amber-rust-s3 | `pull_incremental` | `s3_request_body_bytes` | 0 B |
| amber-rust-s3 | `pull_incremental` | `s3_requests_GET` | 6 |
| amber-rust-s3 | `pull_incremental` | `s3_requests_HEAD` | 1 |
| amber-rust-s3 | `pull_incremental` | `s3_requests_total` | 7 |
| amber-rust-s3 | `pull_incremental` | `s3_response_body_bytes` | 57.46 MiB |
| amber-rust-s3 | `pull_incremental` | `s3_responses_2xx` | 5 |
| amber-rust-s3 | `pull_incremental` | `s3_responses_4xx` | 2 |
| amber-rust-s3 | `pull_incremental` | `stored_bytes` | 698.64 MiB |
| amber-rust-s3 | `pull_incremental` | `stored_objects` | 12 |
| amber-rust-s3 | `retention_cleanup` | `retained_logical_bytes` | 2.03 GiB |
| amber-rust-s3 | `retention_cleanup` | `retained_references` | 1 |
| amber-rust-s3 | `retention_cleanup` | `s3_bytes_down` | 23.94 KiB |
| amber-rust-s3 | `retention_cleanup` | `s3_bytes_up` | 566.05 MiB |
| amber-rust-s3 | `retention_cleanup` | `s3_connections` | 37 |
| amber-rust-s3 | `retention_cleanup` | `s3_request_body_bytes` | 566.00 MiB |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_GET` | 2 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_POST` | 27 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_PUT` | 44 |
| amber-rust-s3 | `retention_cleanup` | `s3_requests_total` | 73 |
| amber-rust-s3 | `retention_cleanup` | `s3_response_body_bytes` | 16.31 KiB |
| amber-rust-s3 | `retention_cleanup` | `s3_responses_2xx` | 72 |
| amber-rust-s3 | `retention_cleanup` | `s3_responses_4xx` | 1 |
| amber-rust-s3 | `retention_cleanup` | `stored_bytes` | 693.33 MiB |
| amber-rust-s3 | `retention_cleanup` | `stored_objects` | 12 |

## Scenario `backup/retention`

### Elapsed time (median, lower is better)

| operation | restic | amber-rust | amber-go |
|---|---|---|---|
| `backup_initial` | 1.73 s | 552.7 ms | 1.41 s |
| `backup_unchanged` | 818.4 ms | 256.5 ms | 709.3 ms |
| `backup_changed` | 1.01 s | 251.2 ms | 808.5 ms |
| `backup_changed_again` | 1.01 s | 212.4 ms | 777.6 ms |
| `list_snapshot` | 757.6 ms | 6.27 s | 5.51 s |
| `restore_latest` | 1.79 s | 2.14 s | 1.72 s |
| `forget_oldest` | 716.2 ms | 79.1 ms | 71.7 ms |
| `prune` | 5.42 s | 333.8 ms | 444.7 ms |

### Throughput over the logical bytes processed (median)

| operation | restic | amber-rust | amber-go |
|---|---|---|---|
| `backup_initial` | 1.18 GiB/s | 3.67 GiB/s | 1.44 GiB/s |
| `backup_unchanged` | 2.48 GiB/s | 7.92 GiB/s | 2.86 GiB/s |
| `backup_changed` | 2.01 GiB/s | 8.08 GiB/s | 2.51 GiB/s |
| `backup_changed_again` | 2.01 GiB/s | 9.56 GiB/s | 2.61 GiB/s |
| `list_snapshot` | — | — | — |
| `restore_latest` | 1.13 GiB/s | 970.87 MiB/s | 1.18 GiB/s |
| `forget_oldest` | — | — | — |
| `prune` | — | — | — |

### CPU time, user + system (median)

| operation | restic | amber-rust | amber-go |
|---|---|---|---|
| `backup_initial` | 23.47 s | 6.95 s | 28.48 s |
| `backup_unchanged` | 651.0 ms | 3.16 s | 11.47 s |
| `backup_changed` | 5.97 s | 3.08 s | 15.57 s |
| `backup_changed_again` | 5.99 s | 3.16 s | 15.57 s |
| `list_snapshot` | 572.1 ms | 6.16 s | 5.82 s |
| `restore_latest` | 2.54 s | 2.13 s | 2.29 s |
| `forget_oldest` | 524.2 ms | 77.7 ms | 75.6 ms |
| `prune` | 6.07 s | 975.3 ms | 1.57 s |

### Peak resident set size (median)

| operation | restic | amber-rust | amber-go |
|---|---|---|---|
| `backup_initial` | 985.90 MiB | 167.62 MiB | 1022.59 MiB |
| `backup_unchanged` | 95.29 MiB | 83.45 MiB | 276.90 MiB |
| `backup_changed` | 434.59 MiB | 83.45 MiB | 427.75 MiB |
| `backup_changed_again` | 439.16 MiB | 83.45 MiB | 420.22 MiB |
| `list_snapshot` | 83.45 MiB | 83.45 MiB | 91.04 MiB |
| `restore_latest` | 291.98 MiB | 649.30 MiB | 671.86 MiB |
| `forget_oldest` | 83.45 MiB | 83.45 MiB | 85.20 MiB |
| `prune` | 329.31 MiB | 553.19 MiB | 676.75 MiB |

### Storage allocated after each operation (median)

| operation | restic | amber-rust | amber-go |
|---|---|---|---|
| `backup_initial` | 707.99 MiB | 691.07 MiB | 694.77 MiB |
| `backup_unchanged` | 707.99 MiB | 691.08 MiB | 694.78 MiB |
| `backup_changed` | 713.92 MiB | 696.76 MiB | 700.46 MiB |
| `backup_changed_again` | 719.78 MiB | 702.38 MiB | 706.07 MiB |
| `list_snapshot` | — | — | — |
| `restore_latest` | — | — | — |
| `forget_oldest` | 719.77 MiB | 702.38 MiB | 706.07 MiB |
| `prune` | 714.23 MiB | 697.11 MiB | 697.32 MiB |

### Space reclaimed (median; positive means released)

| operation | restic | amber-rust | amber-go |
|---|---|---|---|
| `backup_initial` | — | — | — |
| `backup_unchanged` | — | — | — |
| `backup_changed` | — | — | — |
| `backup_changed_again` | — | — | — |
| `list_snapshot` | — | — | — |
| `restore_latest` | — | — | — |
| `forget_oldest` | — | — | — |
| `prune` | 5.55 MiB | 5.27 MiB | 8.74 MiB |

### Dispersion of elapsed time

| backend | operation | n | median | p95 | min | max | stddev | cv |
|---|---|---|---|---|---|---|---|---|
| restic | `backup_initial` | 3 | 1.73 s | 1.73 s | 1.71 s | 1.73 s | 12.1 ms | 0.7% |
| restic | `backup_unchanged` | 3 | 818.4 ms | 822.0 ms | 816.6 ms | 822.0 ms | 2.8 ms | 0.3% |
| restic | `backup_changed` | 3 | 1.01 s | 1.01 s | 1.01 s | 1.01 s | 2.1 ms | 0.2% |
| restic | `backup_changed_again` | 3 | 1.01 s | 1.01 s | 1.01 s | 1.01 s | 1.6 ms | 0.2% |
| restic | `list_snapshot` | 3 | 757.6 ms | 762.9 ms | 751.7 ms | 762.9 ms | 5.6 ms | 0.7% |
| restic | `restore_latest` | 3 | 1.79 s | 1.79 s | 1.78 s | 1.79 s | 7.0 ms | 0.4% |
| restic | `forget_oldest` | 3 | 716.2 ms | 716.2 ms | 715.8 ms | 716.2 ms | 199.6 µs | 0.0% |
| restic | `prune` | 3 | 5.42 s | 5.48 s | 4.78 s | 5.48 s | 383.1 ms | 7.3% |
| amber-rust | `backup_initial` | 3 | 552.7 ms | 595.8 ms | 542.9 ms | 595.8 ms | 28.1 ms | 5.0% |
| amber-rust | `backup_unchanged` | 3 | 256.5 ms | 265.0 ms | 201.5 ms | 265.0 ms | 34.4 ms | 14.3% |
| amber-rust | `backup_changed` | 3 | 251.2 ms | 256.9 ms | 204.7 ms | 256.9 ms | 28.6 ms | 12.0% |
| amber-rust | `backup_changed_again` | 3 | 212.4 ms | 212.5 ms | 212.0 ms | 212.5 ms | 285.0 µs | 0.1% |
| amber-rust | `list_snapshot` | 3 | 6.27 s | 6.49 s | 6.12 s | 6.49 s | 182.7 ms | 2.9% |
| amber-rust | `restore_latest` | 3 | 2.14 s | 2.15 s | 2.14 s | 2.15 s | 4.1 ms | 0.2% |
| amber-rust | `forget_oldest` | 3 | 79.1 ms | 79.4 ms | 75.9 ms | 79.4 ms | 2.0 ms | 2.5% |
| amber-rust | `prune` | 3 | 333.8 ms | 359.6 ms | 299.4 ms | 359.6 ms | 30.2 ms | 9.1% |
| amber-go | `backup_initial` | 3 | 1.41 s | 1.42 s | 1.38 s | 1.42 s | 22.6 ms | 1.6% |
| amber-go | `backup_unchanged` | 3 | 709.3 ms | 751.2 ms | 705.9 ms | 751.2 ms | 25.3 ms | 3.5% |
| amber-go | `backup_changed` | 3 | 808.5 ms | 823.7 ms | 797.7 ms | 823.7 ms | 13.0 ms | 1.6% |
| amber-go | `backup_changed_again` | 3 | 777.6 ms | 800.1 ms | 773.8 ms | 800.1 ms | 14.2 ms | 1.8% |
| amber-go | `list_snapshot` | 3 | 5.51 s | 5.54 s | 5.36 s | 5.54 s | 96.6 ms | 1.8% |
| amber-go | `restore_latest` | 3 | 1.72 s | 1.74 s | 1.72 s | 1.74 s | 13.4 ms | 0.8% |
| amber-go | `forget_oldest` | 3 | 71.7 ms | 72.8 ms | 65.2 ms | 72.8 ms | 4.1 ms | 5.9% |
| amber-go | `prune` | 3 | 444.7 ms | 453.5 ms | 432.3 ms | 453.5 ms | 10.6 ms | 2.4% |

### What these numbers mean, operation by operation

Read these before comparing two cells above: they state what each measured operation actually delivered.

| backend | operation | note |
|---|---|---|
| amber-go | `list_snapshot` | 168 separate `amber-store ls` processes, one per directory: neither core's CLI has a recursive listing |
| amber-go | `prune` | reclaimed_packstore_bytes is the pack-segment bytes the prune released; the whole-store figure also includes the reference database, which compacts itself |
| amber-rust | `list_snapshot` | 168 separate `amber-store ls` processes, one per directory: neither core's CLI has a recursive listing |
| amber-rust | `prune` | reclaimed_packstore_bytes is the pack-segment bytes the prune released; the whole-store figure also includes the reference database, which compacts itself |

### Recorded counters (median per operation)

| backend | operation | counter | median |
|---|---|---|---|
| restic | `backup_initial` | `store_data_allocated_bytes` | 707.67 MiB |
| restic | `backup_initial` | `store_data_apparent_bytes` | 706.58 MiB |
| restic | `backup_initial` | `store_index_allocated_bytes` | 300.00 KiB |
| restic | `backup_initial` | `store_index_apparent_bytes` | 295.26 KiB |
| restic | `backup_initial` | `store_snapshots_allocated_bytes` | 8.00 KiB |
| restic | `backup_initial` | `store_snapshots_apparent_bytes` | 419 B |
| restic | `backup_unchanged` | `store_data_allocated_bytes` | 707.67 MiB |
| restic | `backup_unchanged` | `store_data_apparent_bytes` | 706.58 MiB |
| restic | `backup_unchanged` | `store_index_allocated_bytes` | 300.00 KiB |
| restic | `backup_unchanged` | `store_index_apparent_bytes` | 295.26 KiB |
| restic | `backup_unchanged` | `store_snapshots_allocated_bytes` | 12.00 KiB |
| restic | `backup_unchanged` | `store_snapshots_apparent_bytes` | 871 B |
| restic | `backup_changed` | `store_data_allocated_bytes` | 713.54 MiB |
| restic | `backup_changed` | `store_data_apparent_bytes` | 712.44 MiB |
| restic | `backup_changed` | `store_index_allocated_bytes` | 352.00 KiB |
| restic | `backup_changed` | `store_index_apparent_bytes` | 343.94 KiB |
| restic | `backup_changed` | `store_snapshots_allocated_bytes` | 16.00 KiB |
| restic | `backup_changed` | `store_snapshots_apparent_bytes` | 1.25 KiB |
| restic | `backup_changed_again` | `store_data_allocated_bytes` | 719.35 MiB |
| restic | `backup_changed_again` | `store_data_apparent_bytes` | 718.25 MiB |
| restic | `backup_changed_again` | `store_index_allocated_bytes` | 400.00 KiB |
| restic | `backup_changed_again` | `store_index_apparent_bytes` | 391.23 KiB |
| restic | `backup_changed_again` | `store_snapshots_allocated_bytes` | 20.00 KiB |
| restic | `backup_changed_again` | `store_snapshots_apparent_bytes` | 1.66 KiB |
| restic | `forget_oldest` | `store_data_allocated_bytes` | 719.35 MiB |
| restic | `forget_oldest` | `store_data_apparent_bytes` | 718.25 MiB |
| restic | `forget_oldest` | `store_index_allocated_bytes` | 400.00 KiB |
| restic | `forget_oldest` | `store_index_apparent_bytes` | 391.23 KiB |
| restic | `forget_oldest` | `store_snapshots_allocated_bytes` | 12.00 KiB |
| restic | `forget_oldest` | `store_snapshots_apparent_bytes` | 829 B |
| restic | `prune` | `allocated_after_prune` | 748924928 |
| restic | `prune` | `allocated_before_forget` | 754741248 |
| restic | `prune` | `store_data_allocated_bytes` | 713.86 MiB |
| restic | `prune` | `store_data_apparent_bytes` | 712.76 MiB |
| restic | `prune` | `store_index_allocated_bytes` | 348.00 KiB |
| restic | `prune` | `store_index_apparent_bytes` | 342.44 KiB |
| restic | `prune` | `store_snapshots_allocated_bytes` | 12.00 KiB |
| restic | `prune` | `store_snapshots_apparent_bytes` | 829 B |
| amber-rust | `backup_initial` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `backup_initial` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `backup_initial` | `store_packstore_allocated_bytes` | 689.52 MiB |
| amber-rust | `backup_initial` | `store_packstore_apparent_bytes` | 689.45 MiB |
| amber-rust | `backup_initial` | `store_refs_allocated_bytes` | 1.55 MiB |
| amber-rust | `backup_initial` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `backup_unchanged` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `backup_unchanged` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `backup_unchanged` | `store_packstore_allocated_bytes` | 689.52 MiB |
| amber-rust | `backup_unchanged` | `store_packstore_apparent_bytes` | 689.45 MiB |
| amber-rust | `backup_unchanged` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `backup_unchanged` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `backup_changed` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `backup_changed` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `backup_changed` | `store_packstore_allocated_bytes` | 695.20 MiB |
| amber-rust | `backup_changed` | `store_packstore_apparent_bytes` | 695.13 MiB |
| amber-rust | `backup_changed` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `backup_changed` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `backup_changed_again` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `backup_changed_again` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `backup_changed_again` | `store_packstore_allocated_bytes` | 700.81 MiB |
| amber-rust | `backup_changed_again` | `store_packstore_apparent_bytes` | 700.74 MiB |
| amber-rust | `backup_changed_again` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `backup_changed_again` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `list_snapshot` | `cli_invocations` | 168 |
| amber-rust | `list_snapshot` | `entries` | 5842 |
| amber-rust | `forget_oldest` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `forget_oldest` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `forget_oldest` | `store_packstore_allocated_bytes` | 700.81 MiB |
| amber-rust | `forget_oldest` | `store_packstore_apparent_bytes` | 700.74 MiB |
| amber-rust | `forget_oldest` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `forget_oldest` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-rust | `prune` | `allocated_after_prune` | 730968064 |
| amber-rust | `prune` | `allocated_before_forget` | 736493568 |
| amber-rust | `prune` | `reclaimed_packstore_bytes` | 5.27 MiB |
| amber-rust | `prune` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-rust | `prune` | `store_closures_apparent_bytes` | 0 B |
| amber-rust | `prune` | `store_packstore_allocated_bytes` | 695.54 MiB |
| amber-rust | `prune` | `store_packstore_apparent_bytes` | 695.48 MiB |
| amber-rust | `prune` | `store_refs_allocated_bytes` | 1.56 MiB |
| amber-rust | `prune` | `store_refs_apparent_bytes` | 3.52 MiB |
| amber-go | `backup_initial` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `backup_initial` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `backup_initial` | `store_packstore_allocated_bytes` | 690.35 MiB |
| amber-go | `backup_initial` | `store_packstore_apparent_bytes` | 690.28 MiB |
| amber-go | `backup_initial` | `store_refs_allocated_bytes` | 4.41 MiB |
| amber-go | `backup_initial` | `store_refs_apparent_bytes` | 2.85 KiB |
| amber-go | `backup_unchanged` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `backup_unchanged` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `backup_unchanged` | `store_packstore_allocated_bytes` | 690.35 MiB |
| amber-go | `backup_unchanged` | `store_packstore_apparent_bytes` | 690.28 MiB |
| amber-go | `backup_unchanged` | `store_refs_allocated_bytes` | 4.42 MiB |
| amber-go | `backup_unchanged` | `store_refs_apparent_bytes` | 3.56 KiB |
| amber-go | `backup_changed` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `backup_changed` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `backup_changed` | `store_packstore_allocated_bytes` | 696.02 MiB |
| amber-go | `backup_changed` | `store_packstore_apparent_bytes` | 695.96 MiB |
| amber-go | `backup_changed` | `store_refs_allocated_bytes` | 4.43 MiB |
| amber-go | `backup_changed` | `store_refs_apparent_bytes` | 4.28 KiB |
| amber-go | `backup_changed_again` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `backup_changed_again` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `backup_changed_again` | `store_packstore_allocated_bytes` | 701.64 MiB |
| amber-go | `backup_changed_again` | `store_packstore_apparent_bytes` | 701.57 MiB |
| amber-go | `backup_changed_again` | `store_refs_allocated_bytes` | 4.43 MiB |
| amber-go | `backup_changed_again` | `store_refs_apparent_bytes` | 4.98 KiB |
| amber-go | `list_snapshot` | `cli_invocations` | 168 |
| amber-go | `list_snapshot` | `entries` | 5842 |
| amber-go | `forget_oldest` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `forget_oldest` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `forget_oldest` | `store_packstore_allocated_bytes` | 701.64 MiB |
| amber-go | `forget_oldest` | `store_packstore_apparent_bytes` | 701.57 MiB |
| amber-go | `forget_oldest` | `store_refs_allocated_bytes` | 4.42 MiB |
| amber-go | `forget_oldest` | `store_refs_apparent_bytes` | 4.11 KiB |
| amber-go | `prune` | `allocated_after_prune` | 731197440 |
| amber-go | `prune` | `allocated_before_forget` | 735764480 |
| amber-go | `prune` | `reclaimed_packstore_bytes` | 4.35 MiB |
| amber-go | `prune` | `store_closures_allocated_bytes` | 4.00 KiB |
| amber-go | `prune` | `store_closures_apparent_bytes` | 0 B |
| amber-go | `prune` | `store_packstore_allocated_bytes` | 697.29 MiB |
| amber-go | `prune` | `store_packstore_apparent_bytes` | 697.22 MiB |
| amber-go | `prune` | `store_refs_allocated_bytes` | 28.00 KiB |
| amber-go | `prune` | `store_refs_apparent_bytes` | 4.59 KiB |

## What each backend guarantees

These differences are the reason a single ranking would be meaningless. Read them before comparing any two rows.

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

### `nix-binary-cache` in `blob/nix-closure`

* **Compression:** none in a local store; xz for NARs published to a binary cache
* **Encryption:** none; integrity comes from NAR hashes and optional signatures
* **Durability:** store paths are built or unpacked into a temporary name, made read-only, then registered in the SQLite database
* **Concurrency:** parallel substitution and copying
* **Transport:** Nix's own S3 binary-cache store (`nix copy --to s3://...`), speaking S3 directly to the object store. This is a native, supported transport.
* A Nix store is not a general filesystem-tree store: it holds whole store paths with recorded references, and deduplicates only by whole path (plus optional hard-linking of identical files). Comparisons here are storage-layer comparisons.
* object store: local garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] started by the harness; gateway shaping: none: the loopback path to the local object store was measured as it is
* the two pulls state what they delivered: pull_fresh brings a client that has never seen the store up to everything published at that point, and pull_incremental brings the same client up to date after the incremental push. Both record delivered_references and delivered_logical_bytes, and every delivered reference is verified at the destination.
* the binary cache is published under the key prefix s3://amber-cas-bench/bench/standard-1789242092913944754/nix-closure/nix-binary-cache/rep0/cache — Nix carries a cache's key prefix in the path after the bucket, and the harness checks the keys that actually appeared rather than assuming the URI was honoured

### `amber-rust` in `git/history`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.

### `git` in `tree/lifecycle`

* **Compression:** zlib per object, plus delta compression inside packs
* **Encryption:** none
* **Durability:** loose objects and packs are written then renamed; `core.fsyncObjectFiles` is left at its default
* **Concurrency:** single-threaded for commits; packing uses --threads (left at its default)
* Git stores whole file versions, deltified against other versions at pack time. It records no modification times, so restored trees are compared without them.
* restored trees are not compared on permission bits other than the executable bit: this backend does not store it
* restored trees are not compared on modification times: this backend does not store it
* restored trees are not compared on empty directories: this backend does not store it

### `nix` in `nix/closure`

* **Compression:** none in a local store; xz for NARs published to a binary cache
* **Encryption:** none; integrity comes from NAR hashes and optional signatures
* **Durability:** store paths are built or unpacked into a temporary name, made read-only, then registered in the SQLite database
* **Concurrency:** parallel substitution and copying
* A Nix store is not a general filesystem-tree store: it holds whole store paths with recorded references, and deduplicates only by whole path (plus optional hard-linking of identical files). Comparisons here are storage-layer comparisons.
* closure source: the deterministic fixture closure built by this flake. Generation 1 has 8 paths and generation 2 has 9, sharing 6 of them
* the fixture contains executables, relative and absolute symlinks, a dangling symlink and read-only directories

### `amber-rust` in `nix/closure`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.
* closure source: the deterministic fixture closure built by this flake. Generation 1 has 8 paths and generation 2 has 9, sharing 6 of them
* the fixture contains executables, relative and absolute symlinks, a dangling symlink and read-only directories

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
* delivered set: 39 reference(s) in the initial publication and 13 in the incremental one, 52 in total

### `restic-s3` in `blob/backup-corpus`

* **Compression:** zstd (repository format 2), enabled by default
* **Encryption:** mandatory AES-256-CTR with Poly1305-AES authentication; every byte stored is encrypted and authenticated. This is real work the other backends do not do, and it is the main reason restic's CPU column is not comparable to theirs.
* **Durability:** pack files and index files are written then fsynced; snapshots are committed last
* **Concurrency:** parallel file readers and uploaders
* **Transport:** restic's own S3 backend (minio-go), speaking S3 directly to the object store. This is a native, supported transport.
* restic deduplicates with content-defined chunking, like the Amber cores, but with its own parameters (512 KiB average). The harness does not try to equalise them: they are not exposed on restic's command line.
* object store: local garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] started by the harness; gateway shaping: none: the loopback path to the local object store was measured as it is
* the two pulls state what they delivered: pull_fresh brings a client that has never seen the store up to everything published at that point, and pull_incremental brings the same client up to date after the incremental push. Both record delivered_references and delivered_logical_bytes, and every delivered reference is verified at the destination.

### `restic` in `backup/retention`

* **Compression:** zstd (repository format 2), enabled by default
* **Encryption:** mandatory AES-256-CTR with Poly1305-AES authentication; every byte stored is encrypted and authenticated. This is real work the other backends do not do, and it is the main reason restic's CPU column is not comparable to theirs.
* **Durability:** pack files and index files are written then fsynced; snapshots are committed last
* **Concurrency:** parallel file readers and uploaders
* restic deduplicates with content-defined chunking, like the Amber cores, but with its own parameters (512 KiB average). The harness does not try to equalise them: they are not exposed on restic's command line.

### `amber-rust` in `tree/lifecycle`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.

### `amber-rust` in `backup/retention`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.

### `amber-go` in `tree/lifecycle`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.

### `git` in `git/history`

* **Compression:** zlib per object, plus delta compression inside packs
* **Encryption:** none
* **Durability:** loose objects and packs are written then renamed; `core.fsyncObjectFiles` is left at its default
* **Concurrency:** single-threaded for commits; packing uses --threads (left at its default)
* Git stores whole file versions, deltified against other versions at pack time. It records no modification times, so restored trees are compared without them.
* retained versions are not compared on permission bits other than the executable bit: this backend does not store it
* retained versions are not compared on modification times: this backend does not store it
* retained versions are not compared on empty directories: this backend does not store it

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

### `amber-go` in `git/history`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.

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

### `fs-sha256` in `tree/lifecycle`

* **Compression:** none, by construction
* **Encryption:** none
* **Durability:** objects are written to a temporary name, fsynced and renamed; the reference file is fsynced last
* **Concurrency:** single-threaded, by construction
* This is the control, not a product: whole-file SHA-256 addressing with no chunking and no compression. It shows what the workload costs with every clever technique removed.

### `amber-go` in `backup/retention`

* **Compression:** per-record zstd; content keys hash the uncompressed bytes, so compression never affects addressing
* **Encryption:** none: these cores store plaintext
* **Durability:** pack segments are appended and fsynced on seal; the reference store is a transactional key/value database (redb in Rust, Pebble in Go)
* **Concurrency:** tree building is parallel across --jobs workers; pack writes are batched and deduplicated
* The two cores are byte-compatible at the content-addressing layer: identical trees produce identical root keys, which the harness checks.
* The two stores are not byte-for-byte comparable in size: the reference database is redb in the Rust core and Pebble in the Go core, and redb preallocates several megabytes. The store_packstore_* and store_refs_* counters separate the pack bytes, which is what the storage comparison is about, from the database file.

### `git-bundle-s3` in `blob/source-history`

* **Compression:** zlib per object, plus delta compression inside packs
* **Encryption:** none
* **Durability:** loose objects and packs are written then renamed; `core.fsyncObjectFiles` is left at its default
* **Concurrency:** single-threaded for commits; packing uses --threads (left at its default)
* **Transport:** Git bundle publication: `git bundle create` writes the repository (or an increment of it) to a file, which is then uploaded with the MinIO client. Git has no native protocol for pushing to arbitrary S3 object storage, so the object transfer is the harness's, and the numbers below include the cost of producing the bundle as well as of uploading it.
* Git stores whole file versions, deltified against other versions at pack time. It records no modification times, so restored trees are compared without them.
* object store: local garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] started by the harness; gateway shaping: none: the loopback path to the local object store was measured as it is
* the two pulls state what they delivered: pull_fresh brings a client that has never seen the store up to everything published at that point, and pull_incremental brings the same client up to date after the incremental push. Both record delivered_references and delivered_logical_bytes, and every delivered reference is verified at the destination.

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
* delivered set: 39 reference(s) in the initial publication and 13 in the incremental one, 52 in total

### `restic` in `tree/lifecycle`

* **Compression:** zstd (repository format 2), enabled by default
* **Encryption:** mandatory AES-256-CTR with Poly1305-AES authentication; every byte stored is encrypted and authenticated. This is real work the other backends do not do, and it is the main reason restic's CPU column is not comparable to theirs.
* **Durability:** pack files and index files are written then fsynced; snapshots are committed last
* **Concurrency:** parallel file readers and uploaders
* restic deduplicates with content-defined chunking, like the Amber cores, but with its own parameters (512 KiB average). The harness does not try to equalise them: they are not exposed on restic's command line.

## Unsupported operations

These are not zeroes and not omissions: the backend has no equivalent of the operation.

| scenario | backend | operation | why |
|---|---|---|---|
| `git/history` | amber-rust | `clone_full` | these cores ship no repository-to-repository transfer command; what they can do to publish a store is measured in the blob scenario group as a file-level object-storage transport |
| `git/history` | amber-rust | `fetch_incremental` | these cores ship no incremental fetch command; see the blob scenario group for incremental object-storage publication |
| `nix/closure` | amber-rust | `closure_query` | these cores store trees keyed by content and record no references between roots, so there is no closure to query. The harness keeps the reference graph itself and verifies completeness separately; see the closure_is_complete check. |
| `nix/closure` | amber-rust | `nar_and_reference_registration` | these cores record no NAR hash and no reference list for a stored tree: they address content by their own key and model no edges between roots. The harness therefore recomputes the NAR hash of every restored path with `nix hash path` and compares it with the source store's — see the nar_hashes_match_recomputed check — but nothing in the store under test would have noticed a mismatch. |
| `nix/closure` | amber-rust | `cache_export` | these cores have no binary-cache publication format; the blob scenario group measures what they can do, which is to publish their on-disk pack segments to object storage |
| `nix/closure` | amber-rust | `cache_substitute` | these cores have no binary-cache client; see the blob scenario group |
| `git/history` | amber-go | `clone_full` | these cores ship no repository-to-repository transfer command; what they can do to publish a store is measured in the blob scenario group as a file-level object-storage transport |
| `git/history` | amber-go | `fetch_incremental` | these cores ship no incremental fetch command; see the blob scenario group for incremental object-storage publication |
| `nix/closure` | amber-go | `closure_query` | these cores store trees keyed by content and record no references between roots, so there is no closure to query. The harness keeps the reference graph itself and verifies completeness separately; see the closure_is_complete check. |
| `nix/closure` | amber-go | `nar_and_reference_registration` | these cores record no NAR hash and no reference list for a stored tree: they address content by their own key and model no edges between roots. The harness therefore recomputes the NAR hash of every restored path with `nix hash path` and compares it with the source store's — see the nar_hashes_match_recomputed check — but nothing in the store under test would have noticed a mismatch. |
| `nix/closure` | amber-go | `cache_export` | these cores have no binary-cache publication format; the blob scenario group measures what they can do, which is to publish their on-disk pack segments to object storage |
| `nix/closure` | amber-go | `cache_substitute` | these cores have no binary-cache client; see the blob scenario group |

## Correctness checks

| scenario | backend | check | result | detail |
|---|---|---|---|---|
| `backup/retention` | amber-go | `every_retained_snapshot_matches_manifest` | pass (3/3) | all 2 retained snapshots (snap1, snap2) were restored after retention and matched their manifests byte for byte |
| `backup/retention` | amber-go | `forgotten_snapshots_are_absent` | pass (3/3) | the store lists exactly what retention should have left: retained 2 (snap1, snap2), dropped 2 (snap0, snap0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `backup/retention` | amber-go | `restore_matches_manifest` | pass (3/3) | 6009 entries expected under backup-amber-go-rep0/restore-latest; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `backup/retention` | amber-rust | `every_retained_snapshot_matches_manifest` | pass (3/3) | all 2 retained snapshots (snap1, snap2) were restored after retention and matched their manifests byte for byte |
| `backup/retention` | amber-rust | `forgotten_snapshots_are_absent` | pass (3/3) | the store lists exactly what retention should have left: retained 2 (snap1, snap2), dropped 2 (snap0, snap0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `backup/retention` | amber-rust | `restore_matches_manifest` | pass (3/3) | 6009 entries expected under backup-amber-rust-rep0/restore-latest; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `backup/retention` | restic | `every_retained_snapshot_matches_manifest` | pass (3/3) | all 2 retained snapshots (snap1, snap2) were restored after retention and matched their manifests byte for byte |
| `backup/retention` | restic | `forgotten_snapshots_are_absent` | pass (3/3) | the store lists exactly what retention should have left: retained 2 (snap1, snap2), dropped 2 (snap0, snap0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `backup/retention` | restic | `restore_matches_manifest` | pass (3/3) | 6009 entries expected under backup-restic-rep0/restore-latest; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `blob/backup-corpus` | amber-go-s3 | `dropped_references_are_absent` | pass (3/3) | the store lists exactly what retention should have left: retained 1 (gen1), dropped 1 (gen0); absence was read from the backend's own successful listing, not inferred from a failed command |
| `blob/backup-corpus` | amber-go-s3 | `fresh_pull_restores_correctly` | pass (3/3) | 1 of the 1 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/backup-corpus` | amber-go-s3 | `incremental_pull_restores_correctly` | pass (3/3) | 2 of the 2 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/backup-corpus` | amber-go-s3 | `missing_object_is_detected` | pass (3/3) | after deleting backup-corpus/amber-go-s3/rep0/store/packstore/000000000000000e.seg, the backend failed rather than returning an incomplete result: /nix/store/y73dri4kbmb0jx08kdi93cyyxphjk2d6-amber-store-go-0.0.7-4ed4660657b1/bin/amber-store --store <scratch>/runs/blob-backup-corpus-amber-go-s3-rep0/pulled-probe --segment-size 67108864 export ref:gen1: exited 1 |
| `blob/backup-corpus` | amber-go-s3 | `retained_content_valid_after_cleanup` | pass (3/3) | 1 of the 1 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/backup-corpus` | amber-rust-s3 | `dropped_references_are_absent` | pass (3/3) | the store lists exactly what retention should have left: retained 1 (gen1), dropped 1 (gen0); absence was read from the backend's own successful listing, not inferred from a failed command |
| `blob/backup-corpus` | amber-rust-s3 | `fresh_pull_restores_correctly` | pass (3/3) | 1 of the 1 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/backup-corpus` | amber-rust-s3 | `incremental_pull_restores_correctly` | pass (3/3) | 2 of the 2 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/backup-corpus` | amber-rust-s3 | `missing_object_is_detected` | pass (3/3) | after deleting backup-corpus/amber-rust-s3/rep0/store/packstore/000000000000000a.seg, the backend failed rather than returning an incomplete result: /nix/store/5cqjvnw3zg461xww08q9gj3lwd8v2iz1-amber-store-rust-0.2.0-141df2b0a9a8/bin/amber-store --store <scratch>/runs/blob-backup-corpus-amber-rust-s3-rep0/pulled-probe --segment-size 67108864 export ref:gen1: exited 1 |
| `blob/backup-corpus` | amber-rust-s3 | `retained_content_valid_after_cleanup` | pass (3/3) | 1 of the 1 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/backup-corpus` | restic-s3 | `dropped_snapshots_are_absent` | pass (3/3) | the store lists exactly what retention should have left: retained 1 (g1), dropped 2 (g0, g0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `blob/backup-corpus` | restic-s3 | `earlier_delivery_still_matches_manifest` | pass (3/3) | 6008 entries expected under blob-backup-corpus-restic-s3-rep0/pull-initial; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `blob/backup-corpus` | restic-s3 | `fresh_pull_matches_manifest` | pass (3/3) | 6008 entries expected under blob-backup-corpus-restic-s3-rep0/pull-initial; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `blob/backup-corpus` | restic-s3 | `incremental_pull_matches_manifest` | pass (3/3) | 6009 entries expected under blob-backup-corpus-restic-s3-rep0/pull-incremental; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `blob/backup-corpus` | restic-s3 | `missing_object_is_detected` | pass (3/3) | after deleting backup-corpus/restic-s3/rep0/repo/data/5b/5b700c8106245cacb82a894eacbd0cd493f48ea157db86cb81a6d7fca9a0b101, the backend failed rather than returning an incomplete result: /nix/store/jh7vrxar93sk4m8igr35zdlgrz3nakrk-restic-0.18.1/bin/restic -r s3:http://127.0.0.1:44793/amber-cas-bench/bench/standard-1789242092913944754/backup-corpus/restic-s3/rep0/repo --password-file <scratch>/runs/blob-backup-corpus-restic-s3-rep0/restic-password --cache-dir <scratch>/runs/blob-backup-corpus-restic-s3-rep0/cache-post check --read-data: exited 1 |
| `blob/backup-corpus` | restic-s3 | `retained_content_valid_after_cleanup` | pass (3/3) | 6009 entries expected under blob-backup-corpus-restic-s3-rep0/pull-after-cleanup; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `blob/nix-closure` | amber-go-s3 | `dropped_references_are_absent` | pass (3/3) | the store lists exactly what retention should have left: retained 9 (nixpath-1x77rqmr9s2x5jdlnygsn4bfa3vgk9hr-amber-bench-lib-late-standard, nixpath-3x88dy9i59zayvssd1b5sk0njd96z82c-amber-bench-app-gen2-standard, nixpath-6k21ksl54w8144crxp3dp4y03kmbkgmn-amber-bench-lib-extra-standard, nixpath-8l793lb88gss022girrnbs9b7k2kardz-amber-bench-lib-core-standard, nixpath-a0ky7r815720z4763qy8n6h34qfc90i2-amber-bench-data-text-g2-standard, nixpath-h0a27fdfzccfna4vlb5x6cy9m1iyr8jq-amber-bench-data-blob-g1-standard, nixpath-nilm6pahg962dwql07chaiczjdhhdr1r-amber-bench-mid-a-standard, nixpath-v6jbapgcaf6hsq348sq8jgyrh3pl98k0-amber-bench-mid-b-standard, nixpath-vrp85wzpcbn4a0n1knazkihqxbix47j1-amber-bench-lib-shared-standard), dropped 2 (nixpath-0hl7d9rc6i4f8jrnykisrbgmiafwipj2-amber-bench-data-text-g1-standard, nixpath-sp9w5w9s24sfyp81cnwnggnnspp80b38-amber-bench-app-gen1-standard); absence was read from the backend's own successful listing, not inferred from a failed command |
| `blob/nix-closure` | amber-go-s3 | `fresh_pull_restores_correctly` | pass (3/3) | 8 of the 8 delivered reference(s) restored from the downloaded store and matched byte for byte; 8 of them also reproduced the NAR hash the source Nix store recorded, which is every one of them; 0 unverified |
| `blob/nix-closure` | amber-go-s3 | `incremental_pull_restores_correctly` | pass (3/3) | 17 of the 17 delivered reference(s) restored from the downloaded store and matched byte for byte; 17 of them also reproduced the NAR hash the source Nix store recorded, which is every one of them; 0 unverified |
| `blob/nix-closure` | amber-go-s3 | `missing_object_is_detected` | pass (3/3) | after deleting nix-closure/amber-go-s3/rep0/store/packstore/0000000000000003.seg, the backend failed rather than returning an incomplete result: /nix/store/y73dri4kbmb0jx08kdi93cyyxphjk2d6-amber-store-go-0.0.7-4ed4660657b1/bin/amber-store --store <scratch>/runs/blob-nix-closure-amber-go-s3-rep0/pulled-probe --segment-size 67108864 export ref:nixpath-1x77rqmr9s2x5jdlnygsn4bfa3vgk9hr-amber-bench-lib-late-standard: exited 1 |
| `blob/nix-closure` | amber-go-s3 | `retained_content_valid_after_cleanup` | pass (3/3) | 9 of the 9 delivered reference(s) restored from the downloaded store and matched byte for byte; 9 of them also reproduced the NAR hash the source Nix store recorded, which is every one of them; 0 unverified |
| `blob/nix-closure` | amber-rust-s3 | `dropped_references_are_absent` | pass (3/3) | the store lists exactly what retention should have left: retained 9 (nixpath-1x77rqmr9s2x5jdlnygsn4bfa3vgk9hr-amber-bench-lib-late-standard, nixpath-3x88dy9i59zayvssd1b5sk0njd96z82c-amber-bench-app-gen2-standard, nixpath-6k21ksl54w8144crxp3dp4y03kmbkgmn-amber-bench-lib-extra-standard, nixpath-8l793lb88gss022girrnbs9b7k2kardz-amber-bench-lib-core-standard, nixpath-a0ky7r815720z4763qy8n6h34qfc90i2-amber-bench-data-text-g2-standard, nixpath-h0a27fdfzccfna4vlb5x6cy9m1iyr8jq-amber-bench-data-blob-g1-standard, nixpath-nilm6pahg962dwql07chaiczjdhhdr1r-amber-bench-mid-a-standard, nixpath-v6jbapgcaf6hsq348sq8jgyrh3pl98k0-amber-bench-mid-b-standard, nixpath-vrp85wzpcbn4a0n1knazkihqxbix47j1-amber-bench-lib-shared-standard), dropped 2 (nixpath-0hl7d9rc6i4f8jrnykisrbgmiafwipj2-amber-bench-data-text-g1-standard, nixpath-sp9w5w9s24sfyp81cnwnggnnspp80b38-amber-bench-app-gen1-standard); absence was read from the backend's own successful listing, not inferred from a failed command |
| `blob/nix-closure` | amber-rust-s3 | `fresh_pull_restores_correctly` | pass (3/3) | 8 of the 8 delivered reference(s) restored from the downloaded store and matched byte for byte; 8 of them also reproduced the NAR hash the source Nix store recorded, which is every one of them; 0 unverified |
| `blob/nix-closure` | amber-rust-s3 | `incremental_pull_restores_correctly` | pass (3/3) | 17 of the 17 delivered reference(s) restored from the downloaded store and matched byte for byte; 17 of them also reproduced the NAR hash the source Nix store recorded, which is every one of them; 0 unverified |
| `blob/nix-closure` | amber-rust-s3 | `missing_object_is_detected` | pass (3/3) | after deleting nix-closure/amber-rust-s3/rep0/store/packstore/0000000000000003.seg, the backend failed rather than returning an incomplete result: /nix/store/5cqjvnw3zg461xww08q9gj3lwd8v2iz1-amber-store-rust-0.2.0-141df2b0a9a8/bin/amber-store --store <scratch>/runs/blob-nix-closure-amber-rust-s3-rep0/pulled-probe --segment-size 67108864 export ref:nixpath-1x77rqmr9s2x5jdlnygsn4bfa3vgk9hr-amber-bench-lib-late-standard: exited 1 |
| `blob/nix-closure` | amber-rust-s3 | `retained_content_valid_after_cleanup` | pass (3/3) | 9 of the 9 delivered reference(s) restored from the downloaded store and matched byte for byte; 9 of them also reproduced the NAR hash the source Nix store recorded, which is every one of them; 0 unverified |
| `blob/nix-closure` | nix-binary-cache | `earlier_delivery_still_matches_source` | pass (3/3) | all 8 paths of the closure arrived: the destination registers each one with the source store's NAR hash and reference list, 8 of them reproduce that hash when recomputed from the bytes on disk with `nix hash path`, the destination holds nothing extra, and every reference of every path is inside the delivered set |
| `blob/nix-closure` | nix-binary-cache | `expired_narinfos_are_absent` | pass (3/3) | all 2 expired narinfo object(s) are absent from a successful listing of the 21 object(s) the cache still holds, and a client with its own empty narinfo cache obtains none of the 2 store paths only the superseded generation needed |
| `blob/nix-closure` | nix-binary-cache | `fresh_pull_matches_source` | pass (3/3) | all 8 paths of the closure arrived: the destination registers each one with the source store's NAR hash and reference list, 8 of them reproduce that hash when recomputed from the bytes on disk with `nix hash path`, the destination holds nothing extra, and every reference of every path is inside the delivered set |
| `blob/nix-closure` | nix-binary-cache | `incremental_pull_matches_source` | pass (3/3) | all 9 paths of the closure arrived: the destination registers each one with the source store's NAR hash and reference list, 9 of them reproduce that hash when recomputed from the bytes on disk with `nix hash path`, the destination holds nothing extra, and every reference of every path is inside the delivered set |
| `blob/nix-closure` | nix-binary-cache | `missing_object_is_detected` | pass (3/3) | after deleting nix-closure/nix-binary-cache/rep0/cache/nar/0vw6r818h5ri309y31737fg755cp1gfhalakbsq53l9h18wldwzv.nar.xz, the backend failed rather than returning an incomplete result: /nix/store/0djmqy9c50czxaakvi7xqinz30wlbs6g-nix-2.34.8/bin/nix --extra-experimental-features "nix-command flakes" --option substituters  --option extra-substituters  --option trusted-substituters  --option builders  copy --no-check-sigs --from s3://amber-cas-bench/bench/standard-1789242092913944754/nix-closure/nix-binary-cache/rep0/cache?endpoint=127.0.0.1:44793&region=garage&scheme=http --to <scratch>/runs/blob-nix-closure-nix-binary-cache-rep0/store-probe /nix/store/3x88dy9i59zayvssd1b5sk0njd96z82c-amber-bench-app-gen2-standard: exited 1 |
| `blob/nix-closure` | nix-binary-cache | `published_keys_stay_inside_the_namespace` | pass (3/3) | 17 object(s) appeared and every one of them is under amber-cas-bench/bench/standard-1789242092913944754/nix-closure/nix-binary-cache/rep0/cache, which is what the store URI asked for |
| `blob/nix-closure` | nix-binary-cache | `retained_content_valid_after_cleanup` | pass (3/3) | all 9 paths of the closure arrived: the destination registers each one with the source store's NAR hash and reference list, 9 of them reproduce that hash when recomputed from the bytes on disk with `nix hash path`, the destination holds nothing extra, and every reference of every path is inside the delivered set |
| `blob/source-history` | amber-go-s3 | `dropped_references_are_absent` | pass (3/3) | the store lists exactly what retention should have left: retained 13 (history-0039, history-0040, history-0041, history-0042, history-0043, history-0044, history-0045, history-0046, history-0047, history-0048, history-0049, history-0050, history-0051), dropped 39 (history-0000, history-0001, history-0002, history-0003, history-0004, history-0005, history-0006, history-0007, history-0008, history-0009, history-0010, history-0011, history-0012, history-0013, history-0014, history-0015, history-0016, history-0017, history-0018, history-0019, history-0020, history-0021, history-0022, history-0023, history-0024, history-0025, history-0026, history-0027, history-0028, history-0029, history-0030, history-0031, history-0032, history-0033, history-0034, history-0035, history-0036, history-0037, history-0038); absence was read from the backend's own successful listing, not inferred from a failed command |
| `blob/source-history` | amber-go-s3 | `fresh_pull_restores_correctly` | pass (3/3) | 39 of the 39 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/source-history` | amber-go-s3 | `incremental_pull_restores_correctly` | pass (3/3) | 52 of the 52 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/source-history` | amber-go-s3 | `missing_object_is_detected` | pass (3/3) | after deleting source-history/amber-go-s3/rep0/store/packstore/0000000000000003.seg.active, the backend failed rather than returning an incomplete result: /nix/store/y73dri4kbmb0jx08kdi93cyyxphjk2d6-amber-store-go-0.0.7-4ed4660657b1/bin/amber-store --store <scratch>/runs/blob-source-history-amber-go-s3-rep0/pulled-probe --segment-size 67108864 export ref:history-0039: exited 1 |
| `blob/source-history` | amber-go-s3 | `retained_content_valid_after_cleanup` | pass (3/3) | 13 of the 13 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/source-history` | amber-rust-s3 | `dropped_references_are_absent` | pass (3/3) | the store lists exactly what retention should have left: retained 13 (history-0039, history-0040, history-0041, history-0042, history-0043, history-0044, history-0045, history-0046, history-0047, history-0048, history-0049, history-0050, history-0051), dropped 39 (history-0000, history-0001, history-0002, history-0003, history-0004, history-0005, history-0006, history-0007, history-0008, history-0009, history-0010, history-0011, history-0012, history-0013, history-0014, history-0015, history-0016, history-0017, history-0018, history-0019, history-0020, history-0021, history-0022, history-0023, history-0024, history-0025, history-0026, history-0027, history-0028, history-0029, history-0030, history-0031, history-0032, history-0033, history-0034, history-0035, history-0036, history-0037, history-0038); absence was read from the backend's own successful listing, not inferred from a failed command |
| `blob/source-history` | amber-rust-s3 | `fresh_pull_restores_correctly` | pass (3/3) | 39 of the 39 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/source-history` | amber-rust-s3 | `incremental_pull_restores_correctly` | pass (3/3) | 52 of the 52 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/source-history` | amber-rust-s3 | `missing_object_is_detected` | pass (3/3) | after deleting source-history/amber-rust-s3/rep0/store/packstore/0000000000000003.seg.active, the backend failed rather than returning an incomplete result: /nix/store/5cqjvnw3zg461xww08q9gj3lwd8v2iz1-amber-store-rust-0.2.0-141df2b0a9a8/bin/amber-store --store <scratch>/runs/blob-source-history-amber-rust-s3-rep0/pulled-probe --segment-size 67108864 export ref:history-0039: exited 1 |
| `blob/source-history` | amber-rust-s3 | `retained_content_valid_after_cleanup` | pass (3/3) | 13 of the 13 delivered reference(s) restored from the downloaded store and matched byte for byte, which is every one of them; 0 unverified |
| `blob/source-history` | git-bundle-s3 | `fresh_pull_matches_source` | pass (3/3) | the destination repository passed `git fsck`, holds every branch and tag of the source at the same commit id, and a working tree checked out of it matches the manifest of the delivered history state 38 |
| `blob/source-history` | git-bundle-s3 | `incremental_pull_matches_source` | pass (3/3) | the increment applied to the existing clone, which passed `git fsck`, and a working tree checked out of its main tip matches the manifest of history state 51 |
| `blob/source-history` | git-bundle-s3 | `missing_object_is_detected` | pass (3/3) | after deleting source-history/git-bundle-s3/rep0/bundles/current.bundle, the backend failed rather than returning an incomplete result: /nix/store/0fid83wpnczqbql3y712a5p6f07kzc1p-minio-client-2025-08-13T08-35-41Z/bin/mc --config-dir <scratch>/mc-config --no-color mirror --overwrite bench/amber-cas-bench/bench/standard-1789242092913944754/source-history/git-bundle-s3/rep0/bundles <scratch>/runs/blob-source-history-git-bundle-s3-rep0/download-probe: exited 1 |
| `blob/source-history` | git-bundle-s3 | `retained_content_valid_after_cleanup` | pass (3/3) | the destination repository passed `git fsck`, holds every branch and tag of the source at the same commit id, and a working tree checked out of it matches the manifest of the delivered history state 51 |
| `blob/source-history` | git-bundle-s3 | `superseded_objects_are_absent` | pass (3/3) | the bucket holds the consolidated bundle and none of the superseded ones (1 object(s) remain) |
| `git/history` | amber-go | `all_retained_versions_match` | pass (3/3) | every one of the 52 retained versions was restored and compared byte for byte; 0 unverified |
| `git/history` | amber-go | `head_version_matches_manifest` | pass (3/3) | 1980 entries expected under git-amber-go-rep0/checkout-head; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `git/history` | amber-rust | `all_retained_versions_match` | pass (3/3) | every one of the 52 retained versions was restored and compared byte for byte; 0 unverified |
| `git/history` | amber-rust | `head_version_matches_manifest` | pass (3/3) | 1980 entries expected under git-amber-rust-rep0/checkout-head; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `git/history` | git | `all_retained_versions_match` | pass (3/3) | every one of the 52 retained versions was restored and compared byte for byte; 0 unverified |
| `git/history` | git | `clone_matches_source` | pass (3/3) | the destination repository passed `git fsck`, holds every branch and tag of the source at the same commit id, and a working tree checked out of it matches the manifest of history state 38 |
| `git/history` | git | `head_version_matches_manifest` | pass (3/3) | 1980 entries expected under git-git-rep0/checkout-head; 0 missing, 0 extra, 0 differing (mode=ExecBit, mtime=false, symlinks=true) |
| `git/history` | git | `incremental_fetch_matches_source` | pass (3/3) | the destination repository passed `git fsck`, holds every branch and tag of the source at the same commit id, and a working tree checked out of it matches the manifest of history state 51 |
| `nix/closure` | amber-go | `closure_is_complete` | pass (3/3) | every reference of every retained path is itself retained. These cores store trees and have no notion of references between roots, so this was checked by the harness against the graph recorded from the source store, not by the store under test. |
| `nix/closure` | amber-go | `closure_matches_source` | pass (3/3) | all 9 store paths of generation 2 came back with identical bytes, permissions, symlink targets and timestamps, and the destination holds nothing else |
| `nix/closure` | amber-go | `nar_hashes_match_recomputed` | pass (3/3) | the NAR hash recomputed with `nix hash path` over every one of the 9 restored store paths equals the hash the source Nix store recorded. These cores register no NAR hashes themselves — see the unsupported nar_and_reference_registration operation — so this was recomputed from the restored bytes. |
| `nix/closure` | amber-go | `post_gc_closure_is_complete` | pass (3/3) | every reference of every retained path is itself retained. These cores store trees and have no notion of references between roots, so this was checked by the harness against the graph recorded from the source store, not by the store under test. |
| `nix/closure` | amber-go | `post_gc_closure_matches_source` | pass (3/3) | all 9 store paths of generation 2 came back with identical bytes, permissions, symlink targets and timestamps, and the destination holds nothing else |
| `nix/closure` | amber-go | `post_gc_nar_hashes_match_recomputed` | pass (3/3) | the NAR hash recomputed with `nix hash path` over every one of the 9 restored store paths equals the hash the source Nix store recorded. These cores register no NAR hashes themselves — see the unsupported nar_and_reference_registration operation — so this was recomputed from the restored bytes. |
| `nix/closure` | amber-rust | `closure_is_complete` | pass (3/3) | every reference of every retained path is itself retained. These cores store trees and have no notion of references between roots, so this was checked by the harness against the graph recorded from the source store, not by the store under test. |
| `nix/closure` | amber-rust | `closure_matches_source` | pass (3/3) | all 9 store paths of generation 2 came back with identical bytes, permissions, symlink targets and timestamps, and the destination holds nothing else |
| `nix/closure` | amber-rust | `nar_hashes_match_recomputed` | pass (3/3) | the NAR hash recomputed with `nix hash path` over every one of the 9 restored store paths equals the hash the source Nix store recorded. These cores register no NAR hashes themselves — see the unsupported nar_and_reference_registration operation — so this was recomputed from the restored bytes. |
| `nix/closure` | amber-rust | `post_gc_closure_is_complete` | pass (3/3) | every reference of every retained path is itself retained. These cores store trees and have no notion of references between roots, so this was checked by the harness against the graph recorded from the source store, not by the store under test. |
| `nix/closure` | amber-rust | `post_gc_closure_matches_source` | pass (3/3) | all 9 store paths of generation 2 came back with identical bytes, permissions, symlink targets and timestamps, and the destination holds nothing else |
| `nix/closure` | amber-rust | `post_gc_nar_hashes_match_recomputed` | pass (3/3) | the NAR hash recomputed with `nix hash path` over every one of the 9 restored store paths equals the hash the source Nix store recorded. These cores register no NAR hashes themselves — see the unsupported nar_and_reference_registration operation — so this was recomputed from the restored bytes. |
| `nix/closure` | nix | `closure_is_complete` | pass (3/3) | every reference of every retained path is itself retained; Nix enforces this itself |
| `nix/closure` | nix | `closure_matches_source` | pass (3/3) | all 9 store paths of generation 2 came back with identical bytes, permissions, symlink targets and timestamps, and the destination holds nothing else |
| `nix/closure` | nix | `nar_hashes_and_references_match` | pass (3/3) | all 9 paths of the closure are registered in the destination store with the source store's NAR hash and reference list |
| `nix/closure` | nix | `post_gc_closure_is_complete` | pass (3/3) | every reference of every retained path is itself retained; Nix enforces this itself |
| `nix/closure` | nix | `post_gc_closure_matches_source` | pass (3/3) | all 9 store paths of generation 2 came back with identical bytes, permissions, symlink targets and timestamps, and the destination holds nothing else |
| `nix/closure` | nix | `post_gc_nar_hashes_and_references_match` | pass (3/3) | all 9 paths of the closure are registered in the destination store with the source store's NAR hash and reference list |
| `tree/lifecycle` | amber-go | `integrity_check` | pass (3/3) | the backend's own consistency check passed over the whole store |
| `tree/lifecycle` | amber-go | `post_gc_restore_matches_manifest` | pass (3/3) | 6009 entries expected under tree-amber-go-rep0/restore-gen1; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | amber-go | `restore_matches_manifest` | pass (3/3) | 6008 entries expected under tree-amber-go-rep0/restore-gen0; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | amber-go | `retention_took_effect` | pass (3/3) | the store lists exactly what retention should have left: retained 1 (gen1), dropped 2 (gen0, gen0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `tree/lifecycle` | amber-rust | `integrity_check` | pass (3/3) | the backend's own consistency check passed over the whole store |
| `tree/lifecycle` | amber-rust | `post_gc_restore_matches_manifest` | pass (3/3) | 6009 entries expected under tree-amber-rust-rep0/restore-gen1; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | amber-rust | `restore_matches_manifest` | pass (3/3) | 6008 entries expected under tree-amber-rust-rep0/restore-gen0; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | amber-rust | `retention_took_effect` | pass (3/3) | the store lists exactly what retention should have left: retained 1 (gen1), dropped 2 (gen0, gen0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `tree/lifecycle` | fs-sha256 | `integrity_check` | pass (3/3) | the backend's own consistency check passed over the whole store |
| `tree/lifecycle` | fs-sha256 | `post_gc_restore_matches_manifest` | pass (3/3) | 6009 entries expected under tree-fs-sha256-rep0/restore-gen1; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | fs-sha256 | `restore_matches_manifest` | pass (3/3) | 6008 entries expected under tree-fs-sha256-rep0/restore-gen0; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | fs-sha256 | `retention_took_effect` | pass (3/3) | the store lists exactly what retention should have left: retained 1 (gen1), dropped 2 (gen0, gen0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `tree/lifecycle` | git | `integrity_check` | pass (3/3) | the backend's own consistency check passed over the whole store |
| `tree/lifecycle` | git | `post_gc_restore_matches_manifest` | pass (3/3) | 6009 entries expected under tree-git-rep0/restore-gen1; 0 missing, 0 extra, 0 differing (mode=ExecBit, mtime=false, symlinks=true) |
| `tree/lifecycle` | git | `restore_matches_manifest` | pass (3/3) | 6008 entries expected under tree-git-rep0/restore-gen0; 0 missing, 0 extra, 0 differing (mode=ExecBit, mtime=false, symlinks=true) |
| `tree/lifecycle` | git | `retention_took_effect` | pass (3/3) | the store lists exactly what retention should have left: retained 1 (gen1), dropped 2 (gen0, gen0-again); absence was read from the backend's own successful listing, not inferred from a failed command |
| `tree/lifecycle` | restic | `integrity_check` | pass (3/3) | the backend's own consistency check passed over the whole store |
| `tree/lifecycle` | restic | `post_gc_restore_matches_manifest` | pass (3/3) | 6009 entries expected under tree-restic-rep0/restore-gen1; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | restic | `restore_matches_manifest` | pass (3/3) | 6008 entries expected under tree-restic-rep0/restore-gen0; 0 missing, 0 extra, 0 differing (mode=Full, mtime=true, symlinks=true) |
| `tree/lifecycle` | restic | `retention_took_effect` | pass (3/3) | the store lists exactly what retention should have left: retained 1 (gen1), dropped 2 (gen0, gen0-again); absence was read from the backend's own successful listing, not inferred from a failed command |

## Failures

None.

## Configuration and environment

* Command line: `<repo>/target/release/amber-cas-bench run --harness-repo <repo> --flake-dir <repo> --amber-rust-bin /nix/store/5cqjvnw3zg461xww08q9gj3lwd8v2iz1-amber-store-rust-0.2.0-141df2b0a9a8/bin/amber-store --amber-rust-rev 141df2b0a9a8ad1726769f63811db3a166be188a --amber-go-bin /nix/store/y73dri4kbmb0jx08kdi93cyyxphjk2d6-amber-store-go-0.0.7-4ed4660657b1/bin/amber-store --amber-go-rev 4ed4660657b12421a534ab0b08cfd717ae3d2291 --profile standard --out <run-output-dir>`
* Harness: version 0.1.0, commit `3656de6abe68de2e52a52bde2fb533afefa6730d`
* Chunking, identical for both Amber cores: min 32.00 KiB, average 512.00 KiB, max 1.00 MiB, item-bits 7, xattr-inline-max 256
* Pack segment size, identical for both Amber cores: 64.00 MiB (the cores default to 2 GiB; a smaller, explicit value is used here because a segment is the unit collection reaps, and a profile that stores less than one segment could never reclaim anything)
* Corpus: seed 20260912, 3 generation(s), 2.03 GiB logical bytes in generation 0, manifest digest `06769afa25725ad1d335021f272b2745c92f63bf5339b954fe15387bf3e5af35`
* Source history: 52 commits on 3 branches with 2 tags
* Nix closures: the deterministic fixture closure built by this flake (flake output `nix-fixtures-standard`). Generation 1 closure 8 paths, generation 2 9 paths, 6 shared
* Object store: local garage cargo:1.3.1 [features: k2v, lmdb, sqlite, consul-discovery, kubernetes-discovery, metrics, telemetry-otlp, bundled-libs] started by the harness
* Gateway: every backend's S3 traffic is forwarded verbatim through a counting gateway at http://127.0.0.1:44793; requests are counted by method and bodies by direction, including retries. The harness's own listing and cleanup calls bypass it.
* Network shaping: none: the loopback path to the local object store was measured as it is
* Bucket `amber-cas-bench`, prefix `bench/standard-1789242092913944754`, region `garage`, remote: false. not recorded: access key and secret are passed to child processes in the environment and never written to any output file
* Execution order was randomised per repetition with seed `6624854655743516625`; the exact order is in `report.json` under `execution_order`.
* Host: linux x86_64 on AMD Ryzen 9 7950X3D 16-Core Processor, 32 logical core(s), 125.32 GiB of memory
* Kernel: 6.18.45 #1-NixOS SMP PREEMPT_DYNAMIC Wed Aug 19 16:18:21 UTC 2026
* Load average when the run started: 0.81 1.72 2.13 (1, 5, 15 minutes)
* Scratch: `<scratch>`, 172.21 GiB free at the start

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

