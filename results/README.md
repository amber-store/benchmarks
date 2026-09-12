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

_No run has been published yet._

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
