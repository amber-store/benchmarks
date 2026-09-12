# Recorded benchmark results

Each directory below is one complete run of the suite, exported with
`amber-cas-bench publish` from the report that run wrote. The Markdown
and the SVG charts are readable here with nothing installed; the JSON
and CSV beside them are the same numbers in machine-readable form.

**These are not a leaderboard.** A number is only comparable with
another number from the same scenario, the same run and the same host.
Runs recorded here differ in profile, host and configuration, so rows
of this table must not be compared with each other; open a run and read
its scenarios. Every run states its repetition count, and the small
sample sizes used here mean differences of the same order as the
recorded dispersion are indicative only.

| run | started (UTC) | profile | repeats | read cache | valid | host | harness | cores measured |
|---|---|---|---|---|---|---|---|---|
| [`20260912T194132Z-standard`](20260912T194132Z-standard/README.md) | 2026-09-12T19:41:32Z | `standard` | 3 | warm | yes | 32 × AMD Ryzen 9 7950X3D 16-Core Processor | `3656de6abe68` | amber-go `4ed4660657b1`<br>amber-rust `141df2b0a9a8` |
| [`20260912T193856Z-smoke`](20260912T193856Z-smoke/README.md) | 2026-09-12T19:38:56Z | `smoke` | 2 | warm | yes | 32 × AMD Ryzen 9 7950X3D 16-Core Processor | `3656de6abe68` | amber-go `4ed4660657b1`<br>amber-rust `141df2b0a9a8` |

The *read cache* column is the run's cache policy in one word: `warm`
means the page cache was not dropped, so read and restore numbers
followed the writes that filled the store and must not be read as
cold-cache numbers. Each run's page states the policy in full.

* [`20260912T194132Z-standard`](20260912T194132Z-standard/README.md) — Standard profile on bld1: 3 repetitions of every (scenario, backend) pair, seed 20260912, warm read cache, local Garage object store. The reference measurement recorded here.
* [`20260912T193856Z-smoke`](20260912T193856Z-smoke/README.md) — Smoke verification on bld1: every backend and every scenario in minutes, 2 repetitions, seed 20260913. A correctness gate — the smoke sizes are far too small to compare backends on.

## Exploring the data

`explore.ipynb` in this directory loads any run recorded here and
compares its scenarios. It needs nothing that is not pinned by this
repository's flake:

```
nix develop --command jupyter lab results/explore.ipynb
```

It refuses to draw a performance comparison from a run whose own
verdict is invalid, and it prints its tables as text so a saved copy
is readable without re-running it.
