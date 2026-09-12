#!/usr/bin/env python3
"""Turn the two drivers' raw samples into a report.

The drivers measure; this program decides what may be compared and
recomputes every published number from the raw per-repetition samples. It
never reads a figure another program summarised.

What may be compared is `validate.py`'s question, and it is asked first: if
any of its rules fails, nothing is written at all. What the numbers mean is
this file's question, and it answers it in the units the operation is
actually in -- operations per second, entries per second, payload bytes per
second, encoded bytes per second -- rather than in one unit for everything.

Chart rendering is the shared renderer in ../../python/amber_bench_plot.py,
which takes a finished chart specification and recomputes nothing.
"""
import argparse
import csv
import importlib.util
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(ROOT, 'python'))

import stats  # noqa: E402
import validate as validation  # noqa: E402

REPORT_SCHEMA = 'amber-core-ops/report/2'

# The workload dimensions a scaling plot may sweep, and what each one is
# called in a chart. A sweep is only drawn where one of these varies with
# everything else about the case held fixed.
SWEEPS = [
    ('item_bytes', 'object size (bytes)'),
    ('entries', 'entries'),
    ('depth', 'path depth'),
    ('width', 'directory width'),
    ('objects', 'stored objects'),
    ('files', 'files'),
]

# What a byte count means, spelled out wherever a rate over it is printed.
BYTES_KIND_LABEL = {
    'payload': 'payload bytes',
    'logical-scanned': 'logical bytes scanned',
    'included': 'included payload bytes',
    'encoded': 'encoded bytes',
}


def load(path):
    with open(path) as fh:
        return json.load(fh)


def load_manifest():
    """The coverage matrix, as the expected-case manifest.

    The matrix is the repository's statement about what is measured; the
    report refuses to publish a comparison that does not match it, so a
    workload cannot quietly appear or disappear.
    """
    path = os.path.join(ROOT, 'core-ops', 'coverage', 'matrix.py')
    if not os.path.isfile(path):
        return None
    spec = importlib.util.spec_from_file_location('coverage_matrix', path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.expected_sets()


# ---------------------------------------------------------------------------
# Deriving numbers from samples
# ---------------------------------------------------------------------------

def key_of(s):
    return (s['group'], s['op'], s['workload'], s['threads'])


def collect(doc):
    """Per-case per-repetition series, in every unit the case supports."""
    out = {}
    for s in doc['samples']:
        k = key_of(s)
        e = out.setdefault(k, {
            'ns_per_op': [], 'cpu_ns_per_op': [], 'bytes_per_s': [],
            'ops_per_s': [], 'entries_per_s': [], 'objects_per_s': [],
            'wall_ns': [], 'max_rss_kib': [],
            'bytes': s['bytes'], 'bytes_kind': s.get('bytes_kind', ''),
            'ops': s['ops'], 'dims': s.get('dims', {}),
            'checksum': s.get('checksum'),
        })
        wall = s['wall_ns']
        # Batch-normalised latency: the measured interval divided by the
        # number of core operations it contains. Short operations are
        # batched, so this is the only latency figure that means anything.
        e['ns_per_op'].append(wall / s['ops'])
        e['cpu_ns_per_op'].append((s['cpu_user_ns'] + s['cpu_sys_ns']) / s['ops'])
        e['wall_ns'].append(float(wall))
        e['max_rss_kib'].append(float(s['max_rss_kib']))
        e['ops_per_s'].append(s['ops'] * 1e9 / wall)
        if s['bytes'] > 0:
            e['bytes_per_s'].append(s['bytes'] * 1e9 / wall)
        dims = s.get('dims') or {}
        if dims.get('entries'):
            e['entries_per_s'].append(dims['entries'] * 1e9 / wall)
        if dims.get('objects'):
            e['objects_per_s'].append(dims['objects'] * 1e9 / wall)
    return out


def summarize(cases):
    out = {}
    for k, e in cases.items():
        row = {
            'ops_per_call': e['ops'],
            'bytes_per_call': e['bytes'],
            'bytes_kind': e['bytes_kind'],
            'dims': e['dims'],
            'checksum': e['checksum'],
            'time': stats.describe(e['ns_per_op']),
            'cpu': stats.describe(e['cpu_ns_per_op']),
            'wall': stats.describe(e['wall_ns']),
            'max_rss_kib': stats.describe(e['max_rss_kib']),
            'ops_per_s': stats.describe(e['ops_per_s']),
        }
        # Relative dispersion: the median absolute deviation as a fraction of
        # the median. It is the one variation figure that is comparable
        # between a nanosecond case and a second-long one.
        m = row['time']['median']
        row['variation'] = row['time']['mad'] / m if m else float('nan')
        for name in ('bytes_per_s', 'entries_per_s', 'objects_per_s'):
            if e[name]:
                row[name] = stats.describe(e[name])
        out[k] = row
    return out


def pair(go_cases, rust_cases, go_sum, rust_sum):
    """Paired rows, plus the cases only one core exposes."""
    paired, unpaired = [], []
    for k in sorted(set(go_cases) | set(rust_cases)):
        group, op, workload, threads = k
        if k in go_cases and k in rust_cases:
            a = go_cases[k]['ns_per_op']
            b = rust_cases[k]['ns_per_op']
            u, p, method = stats.mann_whitney_u(a, b)
            lo, hi = stats.bootstrap_ratio_ci(a, b)
            gm = go_sum[k]['time']['median']
            rm = rust_sum[k]['time']['median']
            paired.append({
                'group': group, 'op': op, 'workload': workload, 'threads': threads,
                'dims': go_sum[k]['dims'],
                'go': go_sum[k], 'rust': rust_sum[k],
                'median_ratio_go_over_rust': gm / rm if rm else float('nan'),
                'ratio_ci_low': lo, 'ratio_ci_high': hi,
                'mann_whitney_u': u, 'p_value': p, 'p_method': method,
            })
        else:
            core = 'go' if k in go_cases else 'rust'
            summary = (go_sum if core == 'go' else rust_sum)[k]
            unpaired.append({
                'group': group, 'op': op, 'workload': workload, 'threads': threads,
                'core': core, 'dims': summary['dims'], 'summary': summary,
            })
    return paired, unpaired


def sweep_total(field, r):
    """The swept dimension's total over one measured call.

    `item_bytes` is a per-item size, so the call covers `item_bytes * items`
    of it; every other dimension already describes the whole call. Dividing
    the interval by the right one of those is what makes the per-unit column
    a per-unit column rather than two different things in one table.
    """
    d = r['dims'] or {}
    v = d.get(field, 0)
    if field == 'item_bytes':
        return v * max(d.get('items', 1), 1)
    return v


def sweeps(paired):
    """The scaling series, as the drivers declared them.

    A case that belongs to a curve says so: its dimensions name the swept
    dimension and the series it varies within. Inferring that from the
    numbers would produce curves that are secretly mixtures -- a directory's
    entry count, its width and its stored-object count all move together,
    and "hit" and "miss" lookups have identical dimensions.
    """
    groups = {}
    for r in paired:
        d = r['dims'] or {}
        field = d.get('sweep')
        if not field or not d.get(field):
            continue
        groups.setdefault((r['op'], field, d.get('series', ''), r['threads']), []).append(r)
    out = []
    for (op, field, series, threads), rows in sorted(groups.items()):
        by_value = {}
        for r in rows:
            by_value.setdefault(r['dims'][field], []).append(r)
        # One point per value, or the "curve" is several curves drawn over
        # one another.
        if len(by_value) < 3 or any(len(v) != 1 for v in by_value.values()):
            continue
        label = dict(SWEEPS).get(field, field)
        out.append({
            'op': op, 'dimension': field, 'label': label, 'series': series,
            'threads': threads,
            'points': [(v, by_value[v][0]) for v in sorted(by_value)],
        })
    return out


def worker_scaling(paired):
    """Operations measured at one worker and at the profile's concurrent count."""
    out = []
    by_case = {}
    for r in paired:
        base = r['workload'].replace('jobs-1', 'jobs-*').replace('jobs-N', 'jobs-*')
        base = base.replace('writers-1', 'writers-*').replace('writers-N', 'writers-*')
        if base == r['workload']:
            continue
        by_case.setdefault((r['op'], base), {})[r['threads']] = r
    for (op, base), variants in sorted(by_case.items()):
        if len(variants) != 2:
            continue
        one = variants.get(1)
        many = [v for t, v in variants.items() if t != 1]
        if one is None or not many:
            continue
        many = many[0]
        row = {'op': op, 'workload': base, 'workers': many['threads']}
        for core in ('go', 'rust'):
            t1 = one[core]['time']['median']
            tn = many[core]['time']['median']
            row[core] = {'one_worker_ns': t1, 'n_worker_ns': tn,
                         'speedup': t1 / tn if tn else float('nan')}
        out.append(row)
    return out


# ---------------------------------------------------------------------------
# Formatting
# ---------------------------------------------------------------------------

def fmt_ns(v):
    if v is None or v != v:
        return 'n/a'
    for unit, scale in (('s', 1e9), ('ms', 1e6), ('us', 1e3)):
        if v >= scale:
            return f'{v / scale:.3f} {unit}'
    return f'{v:.1f} ns'


def fmt_rate(v):
    if v is None or v != v:
        return '—'
    return f'{v / (1 << 20):.1f} MiB/s'


def fmt_count_rate(v):
    if v is None or v != v:
        return '—'
    for unit, scale in (('G', 1e9), ('M', 1e6), ('k', 1e3)):
        if v >= scale:
            return f'{v / scale:.2f} {unit}/s'
    return f'{v:.0f} /s'


def fmt_bytes(v):
    for unit, scale in (('GiB', 1 << 30), ('MiB', 1 << 20), ('KiB', 1 << 10)):
        if v >= scale:
            return f'{v / scale:.2f} {unit}'
    return f'{v} B'


def fmt_p(p):
    if p < 1e-4:
        return '<0.0001'
    return f'{p:.4f}'


def fmt_dims(d):
    if not d:
        return ''
    order = ['content', 'item_bytes', 'items', 'entries', 'depth', 'width',
             'shape', 'files', 'objects', 'workers']
    parts = []
    for k in order:
        if d.get(k):
            parts.append(f'{k}={d[k]}')
    return ', '.join(parts)


def thr(r):
    return 'auto' if r['threads'] == 0 else str(r['threads'])


# ---------------------------------------------------------------------------
# Markdown
# ---------------------------------------------------------------------------

def markdown(identity, go, rust, validity, paired, unpaired, sweep_rows,
             worker_rows, plots, manifest):
    p = go['config']
    cores = identity['cores']
    env = identity['environment']
    genv = go['environment']
    L = []
    add = L.append
    add(f'# Amber core operations: Go vs Rust ({p["name"]} profile)')
    add('')
    add('Both cores measured through their **library APIs, in process**. No command')
    add('line is started inside a measured interval.')
    add('')
    add('There is no overall ranking in this report, and none is intended: the two')
    add('implementations trade differently across the operation set, and a single')
    add('number over unlike operations would say nothing.')
    add('')

    add('## Provenance')
    add('')
    add('| | Go core | Rust core |')
    add('|---|---|---|')
    add(f'| Module | `{cores["go"]["module"]}` | `{cores["rust"]["module"]}` |')
    add(f'| Commit | `{cores["go"]["revision"]}` | `{cores["rust"]["revision"]}` |')
    add(f'| Source | {cores["go"]["source"]} | {cores["rust"]["source"]} |')
    add(f'| Working tree dirty | {str(cores["go"]["dirty"]).lower()} | '
        f'{str(cores["rust"]["dirty"]).lower()} |')
    add(f'| Driver SHA-256 | `{cores["go"]["driver_sha256"]}` | '
        f'`{cores["rust"]["driver_sha256"]}` |')
    add(f'| Toolchain | {go["identity"]["toolchain"]} | {rust["identity"]["toolchain"]} |')
    add('')
    add(f'* Pinned by `flake.lock`: Go `{identity["flake_lock"]["amber_go_src"]}`, '
        f'Rust `{identity["flake_lock"]["amber_rust_src"]}`.')
    add(f'* Pin verified before measuring: **{str(identity["pin_verified"]).lower()}**. '
        f'Every build-relevant file of the resolved Go module was compared against the '
        f'pinned checkout (`{cores["go"]["module_tree_vs_pinned_source"]}`), and the Rust '
        f'core was resolved from '
        f'`{cores["rust"].get("cargo_resolved_source", "n/a")}`, whose requested and '
        f'resolved commits are '
        f'`{cores["rust"].get("cargo_lock_requested_rev", "n/a")}` and '
        f'`{cores["rust"].get("cargo_lock_resolved_rev", "n/a")}`.')
    add(f'* Benchmark harness revision `{identity["harness"]["revision"]}` '
        f'(dirty: {str(identity["harness"]["dirty"]).lower()}), '
        f'sources SHA-256 `{identity["harness"]["sources_sha256"]}` — recorded before the run, '
        're-checked after it and before this report was written. The harness revision is '
        'deliberately kept apart from the core revisions above.')
    add('')

    add('## Environment and configuration')
    add('')
    add(f'* Host `{env["host"]}`, {env["kernel"]}, {env["cpu_model"]}, '
        f'{env["cpu_count"]} CPUs, {env["mem_total_kb"] // 1024} MiB RAM.')
    add(f'* Both drivers pinned to CPUs `{env["cpu_set"]}` and run **sequentially**, '
        'never concurrently.')
    add(f'* Operations that choose their own parallelism (marked `auto`) got '
        f'{genv["auto_parallelism"]} workers in both cores, under that CPU set.')
    add(f'* Scratch on `{env["scratch_fs"]}`; extended attributes in fixtures: '
        f'{str(genv["xattrs"]).lower()}.')
    add(f'* Profile `{p["name"]}`: {p["reps"]} measured repetitions after {p["warmup"]} '
        f'warm-up passes, seed `{p["seed"]}`.')
    passes = go.get('passes', 1)
    if passes > 1:
        add(f'* The repetitions were measured in {passes} passes of opposite core order '
            '(Go first, then Rust first), so neither core was systematically measured on '
            'the colder machine.')
    add(f'* Fixtures: {p["corpus_bytes"] >> 20} MiB corpora, {p["tree_files"]} tree files, '
        f'{p["tree_wide"]} wide entries, depth {p["tree_depth"]}, '
        f'{p["store_objects"]} store objects, {p["segment_bytes"] >> 20} MiB segments, '
        f'{p["ref_records"]} references, {p["inbox_packs"]} inbox packs.')
    add(f'* Swept dimensions: payload sizes {", ".join(fmt_bytes(b) for b in (0, 64, 4096, 1 << 20, 8 << 20))} '
        f'crossed with random, compressible and duplicate content; directory widths '
        f'{p["tree_widths"]}; path depths {p["tree_depths"]}; file-index fan-outs '
        f'{p["fan_outs"]}. These are the same absolute points in every profile.')
    add('* Caches were **not** dropped: every measurement is warm. No cache-cold claim is '
        'made anywhere in this report.')
    add(f'* The Go driver runs a full garbage collection before every measured repetition '
        f'(`forced_gc_before_rep`: {str(genv.get("forced_gc_before_rep", False)).lower()}); '
        f'the Rust driver has no collector to run '
        f'({str(rust["environment"].get("forced_gc_before_rep", False)).lower()}). That is '
        'an asymmetry, not a symmetry: it removes the previous case\'s garbage from the Go '
        'measurement and has no counterpart on the other side.')
    add('')

    add('## Validity')
    add('')
    add('None of the tables below exists unless every statement here holds. The rules are')
    add('in `report/validate.py`, and each of them refuses a comparison that would have')
    add('looked plausible.')
    add('')
    for v in validity:
        add(f'* {v}')
    add('')

    add('## Coverage')
    add('')
    add(f'* Paired operation/workload cases: **{len(paired)}**')
    add(f'* Distinct paired operations: **{len({r["op"] for r in paired})}**')
    add(f'* Single-core cases (reported separately, never compared): **{len(unpaired)}**')
    add(f'* Correctness checks: **{len(go["checks"])}** Go, **{len(rust["checks"])}** Rust, '
        f'of which **{len([c for c in go["checks"] if c.get("comparable")])}** assert that '
        'the two cores produced the same output.')
    add(f'* Scaling sweeps drawn: **{len(sweep_rows)}**; worker-scaling pairs: '
        f'**{len(worker_rows)}**.')
    if manifest:
        add(f'* The measured set is exactly the coverage matrix\'s: '
            f'{len(manifest["operations"])} paired operations over '
            f'{sum(len(v) for v in manifest["operations"].values())} workloads. See '
            '`../../core-ops/COVERAGE.md`.')
    add('')

    # -- paired results ----------------------------------------------------
    groups = sorted({r['group'] for r in paired})
    add('## Paired results')
    add('')
    add('`ns/op` is the measured interval divided by the number of core operations in it —')
    add('short operations are batched, so this is the only latency figure that means')
    add('anything. `var` is the median absolute deviation as a fraction of the median: the')
    add('one dispersion figure comparable between a nanosecond case and a second-long one.')
    add('The ratio is Go median ÷ Rust median with a 95 % percentile-bootstrap interval,')
    add('and *p* is a two-sided Mann-Whitney U test over the per-repetition values.')
    add('')
    if p['reps'] < 3:
        add(f'> **This profile measures {p["reps"]} repetition(s) per case.** Fewer than')
        add('> three has no dispersion and no test statistic, so the ratio column below is')
        add('> one observation divided by another and the CI, variation and *p* columns are')
        add('> omitted. The `quick` profile is for checking correctness fast; use')
        add('> `--profile standard` for anything that will be quoted.')
        add('')
    else:
        add(f'> With {p["reps"]} repetitions the interval is wide on purpose and the *p*')
        add('> value is a weak instrument. A ratio whose interval spans 1 is not evidence')
        add('> of a difference, and none of these numbers supports a claim about the two')
        add('> cores in general.')
        add('')
    for g in groups:
        rows = [r for r in paired if r['group'] == g]
        add(f'### `{g}`')
        add('')
        add('| Operation | Workload | Thr | n | Go ns/op | Go var | Rust ns/op | Rust var | '
            'Go/Rust | 95 % CI | p |')
        add('|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|')
        for r in sorted(rows, key=lambda r: (r['op'], r['workload'], r['threads'])):
            if r['go']['time']['n'] < 3:
                ci, pv, gv, rv = '—', '—', '—', '—'
            else:
                ci = f'{r["ratio_ci_low"]:.2f}–{r["ratio_ci_high"]:.2f}'
                pv = fmt_p(r['p_value'])
                gv = f'{r["go"]["variation"]:.1%}'
                rv = f'{r["rust"]["variation"]:.1%}'
            add('| `{op}` | {wl} | {thr} | {n} | {gm} | {gv} | {rm} | {rv} | '
                '{ratio:.2f}× | {ci} | {pv} |'.format(
                    op=r['op'], wl=r['workload'], thr=thr(r), n=r['go']['time']['n'],
                    gm=fmt_ns(r['go']['time']['median']), gv=gv,
                    rm=fmt_ns(r['rust']['time']['median']), rv=rv,
                    ratio=r['median_ratio_go_over_rust'], ci=ci, pv=pv))
        add('')

    # -- rates -------------------------------------------------------------
    add('## Rates')
    add('')
    add('Each operation in the unit it is actually in. `ops/s` counts the core operations')
    add('the case declares; `entries/s` and `objects/s` count the tree entries and stored')
    add('objects its dimensions record, both computed outside the measured interval. The')
    add('byte column names what its bytes are — a metadata walk that never opens a file')
    add('body is measured in *logical bytes scanned*, not in bandwidth.')
    add('')
    add('| Operation | Workload | Bytes counted | Go | Rust | Go ops/s | Rust ops/s | '
        'Go entries/s | Rust entries/s |')
    add('|---|---|---|---:|---:|---:|---:|---:|---:|')
    for r in sorted(paired, key=lambda r: (r['group'], r['op'], r['workload'])):
        g, rr = r['go'], r['rust']
        kind = BYTES_KIND_LABEL.get(g['bytes_kind'], g['bytes_kind'] or '—')
        gb = fmt_rate(g.get('bytes_per_s', {}).get('median')) if 'bytes_per_s' in g else '—'
        rb = fmt_rate(rr.get('bytes_per_s', {}).get('median')) if 'bytes_per_s' in rr else '—'
        ge = fmt_count_rate(g.get('entries_per_s', {}).get('median')) if 'entries_per_s' in g else '—'
        re_ = fmt_count_rate(rr.get('entries_per_s', {}).get('median')) if 'entries_per_s' in rr else '—'
        add(f'| `{r["op"]}` | {r["workload"]} | {kind} | {gb} | {rb} | '
            f'{fmt_count_rate(g["ops_per_s"]["median"])} | '
            f'{fmt_count_rate(rr["ops_per_s"]["median"])} | {ge} | {re_} |')
    add('')

    # -- scaling -----------------------------------------------------------
    if sweep_rows:
        add('## Scaling')
        add('')
        add('Each row is one dimension swept with everything else about the case held')
        add('fixed, so the trend is a statement about that dimension. `per unit` is the')
        add('time divided by the swept dimension at its largest point: where it is flat')
        add('across the row the cost is proportional, where it falls the fixed overhead')
        add('dominated the small end.')
        add('')
        for sw in sweep_rows:
            add(f'### `{sw["op"]}` over {sw["label"]} ({sw["series"]})')
            add('')
            add(f'| {sw["label"]} | Workload | Go ns/op | Rust ns/op | Go/Rust | '
                f'Go ns per {sw["dimension"]} | Rust ns per {sw["dimension"]} |')
            add('|---:|---|---:|---:|---:|---:|---:|')
            for v, r in sw['points']:
                gm = r['go']['time']['median']
                rm = r['rust']['time']['median']
                total = sweep_total(sw['dimension'], r) or 1
                add(f'| {v:,} | {r["workload"]} | {fmt_ns(gm)} | {fmt_ns(rm)} | '
                    f'{r["median_ratio_go_over_rust"]:.2f}× | '
                    f'{fmt_ns(gm * r["go"]["ops_per_call"] / total)} | '
                    f'{fmt_ns(rm * r["rust"]["ops_per_call"] / total)} |')
            add('')

    # -- worker scaling ----------------------------------------------------
    if worker_rows:
        add('## Worker scaling')
        add('')
        add('The same operation asked for one worker and for the profile\'s concurrent')
        add('count. Speed-up is the single-worker time divided by the concurrent time;')
        add('both cores were pinned to the same CPU set.')
        add('')
        add('| Operation | Workload | Workers | Go 1 | Go n | Go speed-up | '
            'Rust 1 | Rust n | Rust speed-up |')
        add('|---|---|---:|---:|---:|---:|---:|---:|---:|')
        for r in worker_rows:
            add(f'| `{r["op"]}` | {r["workload"]} | {r["workers"]} | '
                f'{fmt_ns(r["go"]["one_worker_ns"])} | {fmt_ns(r["go"]["n_worker_ns"])} | '
                f'{r["go"]["speedup"]:.2f}× | '
                f'{fmt_ns(r["rust"]["one_worker_ns"])} | {fmt_ns(r["rust"]["n_worker_ns"])} | '
                f'{r["rust"]["speedup"]:.2f}× |')
        add('')

    # -- CPU ---------------------------------------------------------------
    add('## CPU time')
    add('')
    add('Process CPU time (user + system) attributed to the measured interval, per')
    add('operation. It is the one resource figure the two runtimes report the same')
    add('way; for concurrent cases it exceeds the elapsed time.')
    add('')
    add('| Operation | Workload | Thr | Go CPU ns/op | Rust CPU ns/op | Go/Rust |')
    add('|---|---|---|---:|---:|---:|')
    for r in sorted(paired, key=lambda r: (r['group'], r['op'], r['workload'])):
        gcpu = r['go']['cpu']['median']
        rcpu = r['rust']['cpu']['median']
        ratio = f'{gcpu / rcpu:.2f}×' if rcpu > 0 else '—'
        add(f'| `{r["op"]}` | {r["workload"]} | {thr(r)} | {fmt_ns(gcpu)} | '
            f'{fmt_ns(rcpu)} | {ratio} |')
    add('')

    # -- encoded sizes -----------------------------------------------------
    ge = {(e['op'], e['workload']): e for e in go.get('encodings', [])}
    re_ = {(e['op'], e['workload']): e for e in rust.get('encodings', [])}
    if ge:
        add('## Encoded sizes (per core, never divided)')
        add('')
        add('The two cores compress record payloads with different encoders —')
        add('`klauspost/compress` in Go, libzstd in Rust — so the same input legitimately')
        add('becomes a different number of bytes. Those sizes are side by side here and')
        add('are never used as a shared denominator; the throughput figures above divide')
        add('by the *input*, which is byte-identical.')
        add('')
        add('| Operation | Workload | Items | Input | Go encoded | Rust encoded | Go/Rust |')
        add('|---|---|---:|---:|---:|---:|---:|')
        for k in sorted(ge):
            g, r = ge[k], re_[k]
            ratio = f'{g["encoded_bytes"] / r["encoded_bytes"]:.3f}×' if r['encoded_bytes'] else '—'
            add(f'| `{k[0]}` | {k[1]} | {g["items"]:,} | {fmt_bytes(g["logical_bytes"])} | '
                f'{fmt_bytes(g["encoded_bytes"])} | {fmt_bytes(r["encoded_bytes"])} | {ratio} |')
        add('')

    # -- wire inputs -------------------------------------------------------
    wires = go.get('wire_inputs', [])
    if wires:
        add('## Shared wire inputs')
        add('')
        add('A decoder measured against its own encoder\'s output is measured against a')
        add('different input in each core. These packs are produced once, before anything')
        add('is measured, and **both** drivers read **both** of them: every reader and')
        add('decoder case runs once per producer, over byte-identical input.')
        add('')
        add('| Producer | Objects | Encoded size | SHA-256 |')
        add('|---|---:|---:|---|')
        for w in wires:
            add(f'| {w["producer"]} | {w["objects"]:,} | {fmt_bytes(w["bytes"])} | '
                f'`{w["sha256"]}` |')
        add('')

    if unpaired:
        add('## Single-core operations')
        add('')
        add('These have no counterpart in the other core, which declares them unsupported')
        add('with a reason. They carry a measurement from one side only and are never')
        add('compared. See `COVERAGE.md`.')
        add('')
        add('| Core | Operation | Workload | n | ns/op | var |')
        add('|---|---|---|---:|---:|---:|')
        for r in sorted(unpaired, key=lambda r: (r['core'], r['op'], r['workload'])):
            t = r['summary']['time']
            var = f'{r["summary"]["variation"]:.1%}' if t['n'] >= 3 else '—'
            add(f'| {r["core"]} | `{r["op"]}` | {r["workload"]} | {t["n"]} | '
                f'{fmt_ns(t["median"])} | {var} |')
        add('')

    add('## Runtime counters (not comparable)')
    add('')
    add('The Go runtime accounts for heap allocation; the Rust driver deliberately')
    add('runs with an uninstrumented allocator, because counting allocations there')
    add('would have cost time inside the very intervals being measured. These numbers')
    add('are therefore **Go-only** and are never used in a comparison.')
    add('')
    counters = {}
    for c in go['counters']:
        counters.setdefault((c['op'], c['workload'], c['name']), []).append(c['value'])
    if counters:
        top = sorted(counters.items(), key=lambda kv: -stats.median(kv[1]))[:25]
        add('| Operation | Workload | Counter | Median |')
        add('|---|---|---|---:|')
        for (op, wl, name), vals in top:
            add(f'| `{op}` | {wl} | {name} | {stats.median(vals):,.0f} |')
        add('')
        add(f'The full set ({len(counters)} rows) is in `counters.csv`.')
        add('')

    add('## Resident set')
    add('')
    add('`getrusage` reports one number per process: the high-water resident set over')
    add('the **whole lifetime of the driver**, which only ever grows. It is not a')
    add('per-operation peak and it cannot be attributed to a case — a case measured')
    add('after a large fixture was built inherits that fixture\'s high-water mark. The')
    add('only honest reading is the one below: how much memory each driver had touched')
    add('by the end of its run.')
    add('')
    add(f'* Go driver, end of run: {max(s["max_rss_kib"] for s in go["samples"]):,} KiB')
    add(f'* Rust driver, end of run: {max(s["max_rss_kib"] for s in rust["samples"]):,} KiB')
    add('')
    add('The per-sample values are in `samples.csv` for completeness, under the same')
    add('caveat.')
    add('')

    if plots:
        add('## Charts')
        add('')
        for name in plots:
            add(f'### {name}')
            add('')
            add(f'![{name}](plots/{name}.svg)')
            add('')

    add('## Reproducing')
    add('')
    add('```sh')
    cpus = env['cpu_set'].split('-')
    ncpu = int(cpus[-1]) - int(cpus[0]) + 1
    add(f'./run.sh --profile {p["name"]} --seed {p["seed"]} --cpus {ncpu}')
    add('```')
    add('')
    add('Raw data next to this file: `go-pass*.json`, `rust-pass*.json` (per-repetition')
    add('samples, one document per measurement pass), `samples.csv`, `summary.csv`,')
    add('`paired.csv`, `scaling.csv`, `checks.csv`, `counters.csv`, `encodings.csv`,')
    add('`unsupported.csv`, `report.json`.')
    add('')
    add('## Scope limits')
    add('')
    add('* Measurements are warm. Nothing here says anything about cold-cache behaviour.')
    add(f'* {p["reps"]} repetitions on one host. The dispersion columns are the honest')
    add('  bound on what that supports; differences of the order of the variation column')
    add('  are not differences.')
    add('* `refstore` is Pebble in Go and redb in Rust: the operation is the same, the')
    add('  storage engine is not, and a directory written by one is not readable by the')
    add('  other. Its rows compare two designs, not two implementations of one design.')
    add('* zstd-compressed record payloads come from `klauspost/compress` in Go and')
    add('  libzstd in Rust. Records, segment bodies and wire packs are therefore')
    add('  interoperable but not byte-identical; the decode cases are measured on shared')
    add('  inputs and the encoders\' output sizes are reported separately.')
    add('* Which sealed segment a given object lands in follows the compressed sizes, so')
    add('  per-segment counts differ between the cores by construction. The operations')
    add('  that would otherwise be sensitive to that (`packstore.scan_index`) cover every')
    add('  segment rather than one.')
    add('* Operations that pick their own parallelism are run under one shared CPU set so')
    add('  both cores see the same bound; the effective width is recorded above.')
    add('* The measured interval contains the core call and a constant-time consumption of')
    add('  its output, and nothing else. Fixture construction, store copies and the')
    add('  full-output digests that prove the two cores agree are all outside it.')
    add('* The per-case CPU figures come from `getrusage` for the whole process.')
    return '\n'.join(L) + '\n'


# ---------------------------------------------------------------------------
# CSV
# ---------------------------------------------------------------------------

DIM_FIELDS = ['item_bytes', 'items', 'content', 'entries', 'depth', 'width',
              'shape', 'files', 'objects', 'workers', 'sweep', 'series']


def write_csvs(out, go, rust, paired, unpaired, go_sum, rust_sum, sweep_rows):
    with open(os.path.join(out, 'samples.csv'), 'w', newline='') as fh:
        w = csv.writer(fh)
        w.writerow(['core', 'group', 'op', 'workload', 'threads', 'rep', 'ops', 'bytes',
                    'bytes_kind', 'wall_ns', 'ns_per_op', 'cpu_user_ns', 'cpu_sys_ns',
                    'max_rss_kib', 'checksum'] + DIM_FIELDS)
        for core, doc in (('go', go), ('rust', rust)):
            for s in doc['samples']:
                d = s.get('dims') or {}
                w.writerow([core, s['group'], s['op'], s['workload'], s['threads'], s['rep'],
                            s['ops'], s['bytes'], s.get('bytes_kind', ''), s['wall_ns'],
                            s['wall_ns'] / s['ops'], s['cpu_user_ns'], s['cpu_sys_ns'],
                            s['max_rss_kib'], s.get('checksum', '')]
                           + [d.get(f, '') for f in DIM_FIELDS])

    with open(os.path.join(out, 'summary.csv'), 'w', newline='') as fh:
        w = csv.writer(fh)
        w.writerow(['core', 'group', 'op', 'workload', 'threads', 'n', 'ops_per_call',
                    'bytes_per_call', 'bytes_kind', 'ns_per_op_min', 'ns_per_op_p25',
                    'ns_per_op_median', 'ns_per_op_p75', 'ns_per_op_max', 'ns_per_op_mad',
                    'variation', 'cpu_ns_per_op_median', 'ops_per_s_median',
                    'bytes_per_s_median', 'entries_per_s_median', 'objects_per_s_median',
                    'max_rss_kib_max'] + DIM_FIELDS)
        for core, summ in (('go', go_sum), ('rust', rust_sum)):
            for k in sorted(summ):
                g, op, wl, t = k
                r = summ[k]
                tt = r['time']
                d = r['dims'] or {}
                w.writerow([core, g, op, wl, t, tt['n'], r['ops_per_call'],
                            r['bytes_per_call'], r['bytes_kind'],
                            tt['min'], tt['p25'], tt['median'], tt['p75'], tt['max'],
                            tt['mad'], r['variation'], r['cpu']['median'],
                            r['ops_per_s']['median'],
                            r.get('bytes_per_s', {}).get('median', ''),
                            r.get('entries_per_s', {}).get('median', ''),
                            r.get('objects_per_s', {}).get('median', ''),
                            r['max_rss_kib']['max']]
                           + [d.get(f, '') for f in DIM_FIELDS])

    with open(os.path.join(out, 'paired.csv'), 'w', newline='') as fh:
        w = csv.writer(fh)
        w.writerow(['group', 'op', 'workload', 'threads', 'n', 'go_ns_per_op_median',
                    'go_ns_per_op_mad', 'go_variation', 'rust_ns_per_op_median',
                    'rust_ns_per_op_mad', 'rust_variation',
                    'median_ratio_go_over_rust', 'ratio_ci_low', 'ratio_ci_high',
                    'mann_whitney_u', 'p_value', 'p_method', 'bytes_kind',
                    'go_bytes_per_s_median', 'rust_bytes_per_s_median',
                    'go_ops_per_s_median', 'rust_ops_per_s_median',
                    'go_cpu_ns_per_op_median', 'rust_cpu_ns_per_op_median'] + DIM_FIELDS)
        for r in paired:
            d = r['dims'] or {}
            w.writerow([r['group'], r['op'], r['workload'], r['threads'], r['go']['time']['n'],
                        r['go']['time']['median'], r['go']['time']['mad'], r['go']['variation'],
                        r['rust']['time']['median'], r['rust']['time']['mad'],
                        r['rust']['variation'],
                        r['median_ratio_go_over_rust'], r['ratio_ci_low'], r['ratio_ci_high'],
                        r['mann_whitney_u'], r['p_value'], r['p_method'],
                        r['go']['bytes_kind'],
                        r['go'].get('bytes_per_s', {}).get('median', ''),
                        r['rust'].get('bytes_per_s', {}).get('median', ''),
                        r['go']['ops_per_s']['median'], r['rust']['ops_per_s']['median'],
                        r['go']['cpu']['median'], r['rust']['cpu']['median']]
                       + [d.get(f, '') for f in DIM_FIELDS])

    with open(os.path.join(out, 'scaling.csv'), 'w', newline='') as fh:
        w = csv.writer(fh)
        w.writerow(['op', 'dimension', 'series', 'value', 'workload', 'go_ns_per_op',
                    'rust_ns_per_op', 'ratio_go_over_rust', 'go_ns_per_unit',
                    'rust_ns_per_unit'])
        for sw in sweep_rows:
            for v, r in sw['points']:
                gm, rm = r['go']['time']['median'], r['rust']['time']['median']
                total = sweep_total(sw['dimension'], r) or 1
                w.writerow([sw['op'], sw['dimension'], sw['series'], v, r['workload'], gm, rm,
                            r['median_ratio_go_over_rust'],
                            gm * r['go']['ops_per_call'] / total,
                            rm * r['rust']['ops_per_call'] / total])

    with open(os.path.join(out, 'checks.csv'), 'w', newline='') as fh:
        w = csv.writer(fh)
        w.writerow(['core', 'id', 'group', 'op', 'passed', 'comparable', 'digest', 'detail'])
        for core, doc in (('go', go), ('rust', rust)):
            for c in doc['checks']:
                w.writerow([core, c['id'], c['group'], c['op'], c['passed'],
                            c.get('comparable', False), c.get('digest', ''), c.get('detail', '')])

    with open(os.path.join(out, 'counters.csv'), 'w', newline='') as fh:
        w = csv.writer(fh)
        w.writerow(['core', 'op', 'workload', 'rep', 'name', 'value'])
        for core, doc in (('go', go), ('rust', rust)):
            for c in doc['counters']:
                w.writerow([core, c['op'], c['workload'], c['rep'], c['name'], c['value']])

    with open(os.path.join(out, 'encodings.csv'), 'w', newline='') as fh:
        w = csv.writer(fh)
        w.writerow(['core', 'op', 'workload', 'items', 'logical_bytes', 'encoded_bytes'])
        for core, doc in (('go', go), ('rust', rust)):
            for e in doc.get('encodings', []):
                w.writerow([core, e['op'], e['workload'], e['items'],
                            e['logical_bytes'], e['encoded_bytes']])

    with open(os.path.join(out, 'unsupported.csv'), 'w', newline='') as fh:
        w = csv.writer(fh)
        w.writerow(['core', 'op', 'workload', 'reason'])
        for core, doc in (('go', go), ('rust', rust)):
            for u in doc['unsupported']:
                w.writerow([core, u['op'], u.get('workload', ''), u['reason']])


# ---------------------------------------------------------------------------
# Charts
# ---------------------------------------------------------------------------

def chart_footer(identity, profile):
    return [
        'No overall ranking is implied: the two cores trade differently across the '
        'operation set, and these bars are per operation only.',
        f'Host {identity["environment"]["host"]}, CPUs {identity["environment"]["cpu_set"]}, '
        f'{profile["reps"]} repetitions, warm caches.',
    ]


def group_chart(group, rows, identity, profile, page, pages):
    categories, cells = [], []
    for r in rows:
        t = 'auto' if r['threads'] == 0 else f'{r["threads"]}t'
        categories.append(f'{r["op"].split(".", 1)[1]} — {r["workload"]} [{t}]')
        cells.append([
            {'label': fmt_ns(r['go']['time']['median']), 'value': r['go']['time']['median'],
             'high': r['go']['time']['p75'], 'n': r['go']['time']['n']},
            {'label': fmt_ns(r['rust']['time']['median']), 'value': r['rust']['time']['median'],
             'high': r['rust']['time']['p75'], 'n': r['rust']['time']['n']},
        ])
    lo = min(min(c['value'] for c in row) for row in cells)
    hi = max(max(c['value'] for c in row) for row in cells)
    title = f'{group}: time per operation ({profile["name"]} profile)'
    if pages > 1:
        title += f' — {fmt_ns(lo)} to {fmt_ns(hi)}, page {page} of {pages}'
    return {
        'title': title,
        'subtitle': [
            'Median of the per-repetition values, bar tip to the 75th percentile; '
            'lower is faster. Native library calls, measured in process.',
            f'Go {identity["cores"]["go"]["revision"][:12]} vs '
            f'Rust {identity["cores"]["rust"]["revision"][:12]}, seed {profile["seed"]}.',
        ],
        'footer': chart_footer(identity, profile) + [
            'The bars share one linear axis, so a page only holds operations within '
            'about three decades of one another; the rest of the group is on another page.'
        ] if pages > 1 else chart_footer(identity, profile),
        'categories': categories,
        'series': ['Go core', 'Rust core'],
        'cells': cells,
    }


def paginate_by_magnitude(rows, max_rows=20, max_decades=3.0):
    """Splits a group into pages whose operations share a scale.

    A linear bar chart that puts a 20-nanosecond accessor beside a
    20-millisecond ingest shows one bar and nineteen slivers. Sorting by
    magnitude and cutting where the range would exceed a few decades keeps
    every page readable; the numbers for every case are in the tables
    regardless.
    """
    import math
    rows = sorted(rows, key=lambda r: r['go']['time']['median'])
    pages, current, floor = [], [], None
    for r in rows:
        v = max(r['go']['time']['median'], r['rust']['time']['median'], 1e-9)
        if current and (len(current) >= max_rows
                        or math.log10(v / floor) > max_decades):
            pages.append(current)
            current, floor = [], None
        if not current:
            floor = max(min(r['go']['time']['median'], r['rust']['time']['median']), 1e-9)
        current.append(r)
    if current:
        pages.append(current)
    return pages


def sweep_chart(sw, identity, profile):
    """The scaling curve, drawn as cost per unit of the swept dimension.

    Plotting the absolute time would put a 64-byte point and an 8-MiB point
    on one linear axis, where the small end is invisible. Cost per unit is
    also the question a scaling plot is asked: a flat row means the cost is
    proportional, a falling row means fixed overhead dominated the small end.
    """
    categories, cells = [], []
    for v, r in sw['points']:
        total = sweep_total(sw['dimension'], r) or 1
        categories.append(f'{v:,}')
        row = []
        for core in ('go', 'rust'):
            per = r[core]['time']['median'] * r[core]['ops_per_call'] / total
            row.append({
                'label': f'{fmt_ns(per)} ({fmt_ns(r[core]["time"]["median"])}/op)',
                'value': per,
                'n': r[core]['time']['n'],
            })
        cells.append(row)
    return {
        'title': (f'{sw["op"]}: scaling with {sw["label"]}'
                  + (f', {sw["series"]} content' if sw['series'] else '')
                  + f' ({profile["name"]} profile)'),
        'subtitle': [
            f'Cost per unit of {sw["label"]}: the whole measured call divided by that '
            f'dimension\'s total over the call. Flat means proportional; falling means '
            f'fixed overhead dominated the small end.',
            f'Go {identity["cores"]["go"]["revision"][:12]} vs '
            f'Rust {identity["cores"]["rust"]["revision"][:12]}, seed {profile["seed"]}. '
            f'The bracketed figure is the batch-normalised latency of one operation.',
        ],
        'footer': chart_footer(identity, profile),
        'categories': categories,
        'series': ['Go core', 'Rust core'],
        'cells': cells,
    }


def render_plots(out, identity, profile, paired, sweep_rows):
    try:
        import amber_bench_plot
    except ImportError:
        return []
    plot_dir = os.path.join(out, 'plots')
    # A rerun into the same directory must not leave last time's charts
    # beside this time's: a stale SVG that no table references is a chart of
    # numbers that are no longer in the report.
    if os.path.isdir(plot_dir):
        for name in os.listdir(plot_dir):
            if name.endswith(('.svg', '.json')):
                os.remove(os.path.join(plot_dir, name))
    os.makedirs(plot_dir, exist_ok=True)
    names = []

    def emit(name, spec):
        with open(os.path.join(plot_dir, f'{name}.json'), 'w') as fh:
            json.dump(spec, fh, indent=2, sort_keys=True)
        with open(os.path.join(plot_dir, f'{name}.svg'), 'w') as fh:
            fh.write(amber_bench_plot.render_svg(spec))
        names.append(name)

    for g in sorted({r['group'] for r in paired}):
        rows = [r for r in paired if r['group'] == g]
        pages = paginate_by_magnitude(rows)
        for i, page in enumerate(pages):
            name = g if len(pages) == 1 else f'{g}-{i + 1}'
            emit(name, group_chart(g, page, identity, profile, i + 1, len(pages)))

    for sw in sweep_rows:
        name = (f'scaling-{sw["op"].replace(".", "-")}-{sw["dimension"]}'
                + (f'-{sw["series"]}' if sw['series'] else ''))
        emit(name, sweep_chart(sw, identity, profile))
    return names


# ---------------------------------------------------------------------------

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--identity', required=True)
    ap.add_argument('--go', action='append', required=True,
                    help='a raw Go sample document; repeat once per measurement pass')
    ap.add_argument('--rust', action='append', required=True,
                    help='a raw Rust sample document; repeat once per measurement pass')
    ap.add_argument('--out', required=True)
    ap.add_argument('--no-plots', action='store_true')
    ap.add_argument('--no-manifest', action='store_true',
                    help='skip the coverage-matrix check (for the tests)')
    args = ap.parse_args()

    identity = load(args.identity)
    manifest = None if args.no_manifest else load_manifest()

    try:
        go = validation.merge_passes([load(p) for p in args.go], 'go')
        rust = validation.merge_passes([load(p) for p in args.rust], 'rust')
        validity = validation.validate(identity, go, rust, manifest)
    except validation.Invalid as e:
        print(f'core-ops report: refusing to publish a comparison: {e}', file=sys.stderr)
        sys.exit(1)

    go_cases, rust_cases = collect(go), collect(rust)
    go_sum, rust_sum = summarize(go_cases), summarize(rust_cases)
    paired, unpaired = pair(go_cases, rust_cases, go_sum, rust_sum)
    sweep_rows = sweeps(paired)
    worker_rows = worker_scaling(paired)

    os.makedirs(args.out, exist_ok=True)
    write_csvs(args.out, go, rust, paired, unpaired, go_sum, rust_sum, sweep_rows)
    plots = [] if args.no_plots else render_plots(
        args.out, identity, go['config'], paired, sweep_rows)

    report = {
        'schema': REPORT_SCHEMA,
        'profile': go['config'],
        'identity': identity,
        'validity': validity,
        'paired': paired,
        'unpaired': unpaired,
        'scaling': [{'op': sw['op'], 'dimension': sw['dimension'],
                     'series': sw['series'],
                     'points': [{'value': v, 'workload': r['workload'],
                                 'go_ns_per_op': r['go']['time']['median'],
                                 'rust_ns_per_op': r['rust']['time']['median']}
                                for v, r in sw['points']]}
                    for sw in sweep_rows],
        'worker_scaling': worker_rows,
        # Both cores' checks, tagged: some are single-core by construction
        # (a Rust-only module has Rust-only evidence), so the coverage
        # matrix has to see the union to know its citations were recorded.
        'checks': ([dict(c, core='go') for c in go['checks']]
                   + [dict(c, core='rust') for c in rust['checks']]),
        'encodings': {'go': go.get('encodings', []), 'rust': rust.get('encodings', [])},
        'wire_inputs': {'go': go.get('wire_inputs', []), 'rust': rust.get('wire_inputs', [])},
        'unsupported': {'go': go['unsupported'], 'rust': rust['unsupported']},
        'blackhole': {'go': go['blackhole'], 'rust': rust['blackhole']},
    }
    with open(os.path.join(args.out, 'report.json'), 'w') as fh:
        json.dump(report, fh, indent=2, sort_keys=True, default=str)
    with open(os.path.join(args.out, 'REPORT.md'), 'w') as fh:
        fh.write(markdown(identity, go, rust, validity, paired, unpaired,
                          sweep_rows, worker_rows, plots, manifest))

    print(f'core-ops report: {len(paired)} paired cases, {len(unpaired)} single-core cases, '
          f'{len(sweep_rows)} scaling sweeps, {len(plots)} charts -> {args.out}')


if __name__ == '__main__':
    main()
