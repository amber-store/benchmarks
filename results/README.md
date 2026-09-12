# Recorded Amber core benchmark results

[Standard report](20260912T233829Z/standard/REPORT.md) · [Quick report](20260912T233829Z/quick/REPORT.md)

Both reports include Markdown tables, SVG charts, raw JSON, and CSV data.

| Setting | Value |
|---|---|
| Machine | Dedicated AMD Ryzen 9 7950X3D system |
| CPU allocation | 8 logical CPUs, pinned to CPUs 0–7 |
| Memory | 125.3 GiB visible to Linux |
| Scratch filesystem | ext4 |
| Standard repetitions | 7 per case, split between opposite execution orders |
| Paired cases | 247 |
| Single-core cases | 6 |
| Scaling series | 27 |
| Go core commit | `4ed4660657b12421a534ab0b08cfd717ae3d2291` |
| Rust core commit | `141df2b0a9a8ad1726769f63811db3a166be188a` |

The raw hostname `bld1` identifies this measurement machine.

[identity.json](20260912T233829Z/identity.json) records the clean harness commit, verified core sources, and executable hashes.
Each report also records the post-processing source hash.

All 183 Go and 184 Rust correctness checks passed in each standard pass.
The reports validate identical workload counts, configuration, and comparable output digests.
An independent audit checked 3,242 median, rate, and ratio calculations across both profiles.

Timings use warm caches. RSS describes the process high-water mark, not memory used by one operation.
Operation counts represent logical work units: node entries, ingested files, or individual requests, as applicable.
Physical store layout can differ between implementations and independently built fixtures.

These results compare native core APIs. They contain no Git, Nix-store, blob-storage, or backup-system comparison.
