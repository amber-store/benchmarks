#!/usr/bin/env python3
"""Turn the two drivers' raw samples into a report.

The drivers measure; this program decides what may be compared and recomputes
every published number from the raw per-repetition samples. It refuses to
publish a comparison at all unless:

  * both runs report the same profile and configuration,
  * every correctness check in both runs passed,
  * every cross-core digest the drivers marked comparable is equal,
  * both cores' source revisions are recorded, and
  * nothing that a core declared unsupported produced a sample.

Chart rendering is shared with the cross-system suite's renderer
(../../python/amber_bench_plot.py), which takes a finished chart
specification and recomputes nothing.
"""
import argparse
import csv
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(ROOT, 'python'))

import stats  # noqa: E402

SCHEMA = 'amber-core-ops/samples/1'
REPORT_SCHEMA = 'amber-core-ops/report/1'

# The configuration fields that must be identical on both sides. A difference
# in any of them means the two cores were not asked the same question.
CONFIG_FIELDS = [
    'name', 'seed', 'reps', 'warmup', 'threads_single', 'threads_multi',
    'corpus_bytes', 'tree_files', 'tree_wide', 'tree_depth', 'synthetic_wide',
    'store_objects', 'segment_bytes', 'ref_records', 'inbox_packs', 'batch_ops',
]


def load(path):
    with open(path) as fh:
        return json.load(fh)


class Invalid(Exception):
    """A reason the two runs must not be compared."""


def validate(identity, go, rust):
    """Returns the list of validity statements, raising on the first failure."""
    ok = []

    for name, doc in (('go', go), ('rust', rust)):
        if doc.get('schema') != SCHEMA:
            raise Invalid(f'{name} run has schema {doc.get("schema")!r}, expected {SCHEMA!r}')
        if doc.get('core') != name:
            raise Invalid(f'{name} run declares core {doc.get("core")!r}')
    ok.append('both runs carry the shared raw-sample schema')

    gc_, rc = go['config'], rust['config']
    for field in CONFIG_FIELDS:
        if gc_.get(field) != rc.get(field):
            raise Invalid(
                f'the two runs disagree on {field}: {gc_.get(field)!r} vs {rc.get(field)!r}')
    ok.append('both runs used the same profile and configuration')

    cores = identity['cores']
    for name in ('go', 'rust'):
        rev = cores[name]['revision']
        if not (isinstance(rev, str) and len(rev) == 40):
            raise Invalid(f'{name} core has no recorded 40-character revision')
        if not cores[name].get('driver_sha256'):
            raise Invalid(f'{name} driver has no recorded executable hash')
    ok.append('both cores are identified by a full commit id and an executable hash')

    for name, doc in (('go', go), ('rust', rust)):
        failed = [c['id'] for c in doc['checks'] if not c['passed']]
        if failed:
            raise Invalid(f'{name} run has failing checks: {", ".join(sorted(failed))}')
    ok.append(f'every correctness check passed ({len(go["checks"])} Go, {len(rust["checks"])} Rust)')

    gcomp = {c['id']: c['digest'] for c in go['checks'] if c.get('comparable')}
    rcomp = {c['id']: c['digest'] for c in rust['checks'] if c.get('comparable')}
    only_go = sorted(set(gcomp) - set(rcomp))
    only_rust = sorted(set(rcomp) - set(gcomp))
    if only_go or only_rust:
        raise Invalid(
            'a check marked comparable exists in one core only: '
            f'go-only={only_go} rust-only={only_rust}')
    mismatched = sorted(k for k in gcomp if gcomp[k] != rcomp[k])
    if mismatched:
        raise Invalid('cross-core digests differ: ' + ', '.join(mismatched))
    ok.append(f'all {len(gcomp)} cross-core output digests are identical')

    for name, doc in (('go', go), ('rust', rust)):
        unsupported = {u['op'] for u in doc['unsupported']}
        measured = {s['op'] for s in doc['samples']}
        overlap = sorted(unsupported & measured)
        if overlap:
            raise Invalid(f'{name} both declares and measures: {", ".join(overlap)}')
    ok.append('no core both declares an operation unsupported and measures it')

    for name, doc in (('go', go), ('rust', rust)):
        for s in doc['samples']:
            if s['status'] != 'ok':
                raise Invalid(f'{name} sample {s["op"]}/{s["workload"]} has status {s["status"]}')
            if s['wall_ns'] <= 0 or s['ops'] <= 0:
                raise Invalid(
                    f'{name} sample {s["op"]}/{s["workload"]} has a non-positive '
                    f'time or operation count')
    ok.append('every sample records a positive elapsed time and operation count')

    reps = gc_['reps']
    for name, doc in (('go', go), ('rust', rust)):
        counts = {}
        for s in doc['samples']:
            counts[(s['op'], s['workload'])] = counts.get((s['op'], s['workload']), 0) + 1
        short = sorted(k for k, v in counts.items() if v != reps)
        if short:
            raise Invalid(f'{name} has cases with other than {reps} repetitions: {short[:5]}')
    ok.append(f'every case has exactly {reps} repetitions on both sides')

    if not identity.get('pin_verified'):
        ok.append('WARNING: the run did not verify against the pinned core revisions')
    return ok


def key_of(s):
    return (s['group'], s['op'], s['workload'], s['threads'])


def collect(doc):
    """Per-case per-operation times, CPU times and throughputs."""
    out = {}
    for s in doc['samples']:
        k = key_of(s)
        e = out.setdefault(k, {'ns_per_op': [], 'cpu_ns_per_op': [], 'bytes_per_s': [],
                               'wall_ns': [], 'max_rss_kib': [], 'bytes': s['bytes'],
                               'ops': s['ops']})
        e['ns_per_op'].append(s['wall_ns'] / s['ops'])
        e['cpu_ns_per_op'].append((s['cpu_user_ns'] + s['cpu_sys_ns']) / s['ops'])
        e['wall_ns'].append(float(s['wall_ns']))
        e['max_rss_kib'].append(float(s['max_rss_kib']))
        if s['bytes'] > 0:
            e['bytes_per_s'].append(s['bytes'] * 1e9 / s['wall_ns'])
    return out


def summarize(cases):
    out = {}
    for k, e in cases.items():
        row = {
            'ops_per_call': e['ops'],
            'bytes_per_call': e['bytes'],
            'time': stats.describe(e['ns_per_op']),
            'cpu': stats.describe(e['cpu_ns_per_op']),
            'wall': stats.describe(e['wall_ns']),
            'max_rss_kib': stats.describe(e['max_rss_kib']),
        }
        if e['bytes_per_s']:
            row['throughput'] = stats.describe(e['bytes_per_s'])
        out[k] = row
    return out


def pair(go_cases, rust_cases, go_sum, rust_sum):
    """Paired rows, plus the operations only one core exposes."""
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
                'core': core, 'summary': summary,
            })
    return paired, unpaired


def fmt_ns(v):
    if v != v:
        return 'n/a'
    for unit, scale in (('s', 1e9), ('ms', 1e6), ('us', 1e3)):
        if v >= scale:
            return f'{v / scale:.3f} {unit}'
    return f'{v:.1f} ns'


def fmt_rate(v):
    if v is None or v != v:
        return '—'
    return f'{v / (1 << 20):.1f} MiB/s'


def fmt_p(p):
    if p < 1e-4:
        return '<0.0001'
    return f'{p:.4f}'


def markdown(identity, go, rust, validity, paired, unpaired, go_doc, rust_doc, plots):
    p = go['config']
    cores = identity['cores']
    env = identity['environment']
    L = []
    add = L.append
    add(f'# Amber core operations: Go vs Rust ({p["name"]} profile)')
    add('')
    add('Both cores measured through their **library APIs, in process**. No command')
    add('line is started inside a measured interval, so nothing here is comparable to')
    add('the cross-system CAS suite in `../../results/`, which measures whole CLI runs.')
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
    add(f'* Pin verified before measuring: **{str(identity["pin_verified"]).lower()}** '
        f'(Go module tree vs pinned source: '
        f'`{cores["go"]["module_tree_vs_pinned_source"]}`; '
        f'Rust `Cargo.lock` rev: `{cores["rust"]["cargo_lock_rev"]}`).')
    add(f'* Benchmark harness revision `{identity["harness"]["revision"]}` '
        f'(dirty: {str(identity["harness"]["dirty"]).lower()}), '
        f'sources SHA-256 `{identity["harness"]["sources_sha256"]}` — recorded before the run '
        'and re-checked after it. The harness revision is deliberately kept apart from the '
        'core revisions above.')
    add('')

    add('## Environment and configuration')
    add('')
    add(f'* Host `{env["host"]}`, {env["kernel"]}, {env["cpu_model"]}, '
        f'{env["cpu_count"]} CPUs, {env["mem_total_kb"] // 1024} MiB RAM.')
    add(f'* Both drivers pinned to CPUs `{env["cpu_set"]}` and run **sequentially**, '
        'never concurrently.')
    add(f'* Scratch on `{env["scratch_fs"]}` (`{env["scratch"]}`); '
        f'extended attributes in fixtures: {str(go["environment"]["xattrs"]).lower()}.')
    add(f'* Profile `{p["name"]}`: {p["reps"]} measured repetitions after {p["warmup"]} '
        f'warm-up passes, seed `{p["seed"]}`.')
    add(f'* Fixtures: {p["corpus_bytes"] >> 20} MiB corpora, {p["tree_files"]} tree files, '
        f'{p["tree_wide"]} wide entries, depth {p["tree_depth"]}, '
        f'{p["synthetic_wide"]} synthetic directory entries, '
        f'{p["store_objects"]} store objects, {p["segment_bytes"] >> 20} MiB segments, '
        f'{p["ref_records"]} references, {p["inbox_packs"]} inbox packs.')
    add(f'* Thread counts: single-threaded cases use 1 worker, concurrent cases use '
        f'{p["threads_multi"]}. Operations that choose their own parallelism are marked '
        '`auto` and see the same CPU set in both cores.')
    add('* Caches were **not** dropped: every measurement is warm. No cache-cold claim is '
        'made anywhere in this report.')
    add('')

    add('## Validity')
    add('')
    for v in validity:
        add(f'* {v}')
    add('')

    add('## Coverage')
    add('')
    add(f'* Paired operation/workload cases: **{len(paired)}**')
    add(f'* Distinct paired operations: **{len({r["op"] for r in paired})}**')
    add(f'* Single-core cases (reported separately, never compared): **{len(unpaired)}**')
    add(f'* Operations one core does not export: '
        f'{len(go_doc["unsupported"])} Go-side notes, {len(rust_doc["unsupported"])} Rust-side notes '
        '(see `unsupported.csv` and `../../core-ops/COVERAGE.md`).')
    add('')

    groups = sorted({r['group'] for r in paired})
    add('## Paired results')
    add('')
    add('`ns/op` is the median time of one core operation, recomputed from the raw')
    add('samples; `MAD` is the median absolute deviation over the repetitions. The')
    add('ratio is Go median ÷ Rust median with a 95 % percentile-bootstrap interval,')
    add('and *p* is a two-sided Mann-Whitney U test over the per-repetition values.')
    add('With a handful of repetitions the interval is wide on purpose.')
    add('')
    if p['reps'] < 3:
        add(f'> **This is the `{p["name"]}` profile: {p["reps"]} repetition per case.** It exists')
        add('> to check correctness quickly, not to measure. A single repetition has no')
        add('> dispersion and no test statistic, so the ratio column below is one')
        add('> observation divided by another and the CI and *p* columns are omitted.')
        add('> Use `--profile standard` for anything that will be quoted.')
        add('')
    for g in groups:
        rows = [r for r in paired if r['group'] == g]
        add(f'### `{g}`')
        add('')
        add('| Operation | Workload | Thr | n | Go ns/op | Go MAD | Rust ns/op | Rust MAD | '
            'Go/Rust | 95 % CI | p |')
        add('|---|---|---|---|---:|---:|---:|---:|---:|---|---:|')
        for r in sorted(rows, key=lambda r: (r['op'], r['workload'], r['threads'])):
            thr = 'auto' if r['threads'] == 0 else str(r['threads'])
            if r['go']['time']['n'] < 3:
                ci, pv = '—', '—'
            else:
                ci = f'{r["ratio_ci_low"]:.2f}–{r["ratio_ci_high"]:.2f}'
                pv = fmt_p(r['p_value'])
            add('| `{op}` | {wl} | {thr} | {n} | {gm} | {gmad} | {rm} | {rmad} | '
                '{ratio:.2f}× | {ci} | {pv} |'.format(
                    op=r['op'], wl=r['workload'], thr=thr, n=r['go']['time']['n'],
                    gm=fmt_ns(r['go']['time']['median']),
                    gmad=fmt_ns(r['go']['time']['mad']),
                    rm=fmt_ns(r['rust']['time']['median']),
                    rmad=fmt_ns(r['rust']['time']['mad']),
                    ratio=r['median_ratio_go_over_rust'], ci=ci, pv=pv))
        add('')

    thr_rows = [r for r in paired if 'throughput' in r['go'] and 'throughput' in r['rust']]
    if thr_rows:
        add('## Throughput')
        add('')
        add('Only the cases that move a well-defined number of payload bytes.')
        add('')
        add('| Operation | Workload | Bytes/call | Go | Rust | Go/Rust |')
        add('|---|---|---:|---:|---:|---:|')
        for r in sorted(thr_rows, key=lambda r: (r['group'], r['op'], r['workload'])):
            gt = r['go']['throughput']['median']
            rt = r['rust']['throughput']['median']
            add(f'| `{r["op"]}` | {r["workload"]} | {r["go"]["bytes_per_call"]:,} | '
                f'{fmt_rate(gt)} | {fmt_rate(rt)} | {gt / rt:.2f}× |')
        add('')

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
        thr = 'auto' if r['threads'] == 0 else str(r['threads'])
        add(f'| `{r["op"]}` | {r["workload"]} | {thr} | {fmt_ns(gcpu)} | {fmt_ns(rcpu)} | {ratio} |')
    add('')

    if unpaired:
        add('## Single-core operations')
        add('')
        add('These have no counterpart in the other core, so they carry a measurement')
        add('from one side only and are never compared. See `COVERAGE.md` for why.')
        add('')
        add('| Core | Operation | Workload | n | ns/op | MAD |')
        add('|---|---|---|---:|---:|---:|')
        for r in sorted(unpaired, key=lambda r: (r['core'], r['op'], r['workload'])):
            t = r['summary']['time']
            add(f'| {r["core"]} | `{r["op"]}` | {r["workload"]} | {t["n"]} | '
                f'{fmt_ns(t["median"])} | {fmt_ns(t["mad"])} |')
        add('')

    add('## Runtime counters (not comparable)')
    add('')
    add('The Go runtime accounts for heap allocation; the Rust driver deliberately')
    add('runs with an uninstrumented allocator, because counting allocations there')
    add('would have cost time inside the very intervals being measured. These numbers')
    add('are therefore **Go-only** and are never used in a comparison.')
    add('')
    counters = {}
    for c in go_doc['counters']:
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

    add('## Peak resident set')
    add('')
    add('Process high-water RSS after each case, in KiB. It is a whole-process figure')
    add('that only ever grows, so it bounds a case rather than attributing memory to')
    add('it; read it as "the driver had not exceeded this by the end of the case".')
    add('')
    add(f'* Go driver: {max(s["max_rss_kib"] for s in go_doc["samples"]):,} KiB')
    add(f'* Rust driver: {max(s["max_rss_kib"] for s in rust_doc["samples"]):,} KiB')
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
    add(f'./core-ops/run.sh --profile {p["name"]} --seed {p["seed"]} '
        f'--cpus {len(range(int(env["cpu_set"].split("-")[0]), int(env["cpu_set"].split("-")[1]) + 1))}')
    add('```')
    add('')
    add('Raw data next to this file: `go.json`, `rust.json` (per-repetition samples),')
    add('`samples.csv`, `summary.csv`, `paired.csv`, `checks.csv`, `counters.csv`,')
    add('`unsupported.csv`, `report.json`.')
    add('')
    add('## Scope limits')
    add('')
    add('* Measurements are warm. Nothing here says anything about cold-cache behaviour.')
    add('* `refstore` is Pebble in Go and redb in Rust: the operation is the same, the')
    add('  storage engine is not, and a directory written by one is not readable by the')
    add('  other. Its rows compare two designs, not two implementations of one design.')
    add('* zstd-compressed record payloads come from `klauspost/compress` in Go and')
    add('  libzstd in Rust. Records, segment bodies and wire packs are therefore')
    add('  interoperable but not byte-identical, and the digests that cover them are')
    add('  recorded as within-core anchors rather than cross-core equalities.')
    add('* Which sealed segment a given object lands in follows the compressed sizes,')
    add('  so per-segment counts differ between the cores by construction.')
    add('* Operations that pick their own parallelism (`fstree.reachable_keys`,')
    add('  `fstree.check_complete` at its default) are run under one shared CPU set so')
    add('  both cores see the same bound, but neither exposes a knob to fix it exactly.')
    add('* The per-case CPU and RSS figures come from `getrusage` for the whole process.')
    add('')
    return '\n'.join(L) + '\n'


def write_csvs(out, go_doc, rust_doc, paired, unpaired, go_sum, rust_sum):
    with open(os.path.join(out, 'samples.csv'), 'w', newline='') as fh:
        w = csv.writer(fh)
        w.writerow(['core', 'group', 'op', 'workload', 'threads', 'rep', 'ops', 'bytes',
                    'wall_ns', 'ns_per_op', 'cpu_user_ns', 'cpu_sys_ns', 'max_rss_kib'])
        for core, doc in (('go', go_doc), ('rust', rust_doc)):
            for s in doc['samples']:
                w.writerow([core, s['group'], s['op'], s['workload'], s['threads'], s['rep'],
                            s['ops'], s['bytes'], s['wall_ns'], s['wall_ns'] / s['ops'],
                            s['cpu_user_ns'], s['cpu_sys_ns'], s['max_rss_kib']])

    with open(os.path.join(out, 'summary.csv'), 'w', newline='') as fh:
        w = csv.writer(fh)
        w.writerow(['core', 'group', 'op', 'workload', 'threads', 'n', 'ops_per_call',
                    'bytes_per_call', 'ns_per_op_min', 'ns_per_op_p25', 'ns_per_op_median',
                    'ns_per_op_p75', 'ns_per_op_max', 'ns_per_op_mad',
                    'cpu_ns_per_op_median', 'bytes_per_s_median', 'max_rss_kib_max'])
        for core, summ in (('go', go_sum), ('rust', rust_sum)):
            for k in sorted(summ):
                g, op, wl, thr = k
                r = summ[k]
                t = r['time']
                w.writerow([core, g, op, wl, thr, t['n'], r['ops_per_call'], r['bytes_per_call'],
                            t['min'], t['p25'], t['median'], t['p75'], t['max'], t['mad'],
                            r['cpu']['median'],
                            r.get('throughput', {}).get('median', ''),
                            r['max_rss_kib']['max']])

    with open(os.path.join(out, 'paired.csv'), 'w', newline='') as fh:
        w = csv.writer(fh)
        w.writerow(['group', 'op', 'workload', 'threads', 'n', 'go_ns_per_op_median',
                    'go_ns_per_op_mad', 'rust_ns_per_op_median', 'rust_ns_per_op_mad',
                    'median_ratio_go_over_rust', 'ratio_ci_low', 'ratio_ci_high',
                    'mann_whitney_u', 'p_value', 'p_method',
                    'go_bytes_per_s_median', 'rust_bytes_per_s_median',
                    'go_cpu_ns_per_op_median', 'rust_cpu_ns_per_op_median'])
        for r in paired:
            w.writerow([r['group'], r['op'], r['workload'], r['threads'], r['go']['time']['n'],
                        r['go']['time']['median'], r['go']['time']['mad'],
                        r['rust']['time']['median'], r['rust']['time']['mad'],
                        r['median_ratio_go_over_rust'], r['ratio_ci_low'], r['ratio_ci_high'],
                        r['mann_whitney_u'], r['p_value'], r['p_method'],
                        r['go'].get('throughput', {}).get('median', ''),
                        r['rust'].get('throughput', {}).get('median', ''),
                        r['go']['cpu']['median'], r['rust']['cpu']['median']])

    with open(os.path.join(out, 'checks.csv'), 'w', newline='') as fh:
        w = csv.writer(fh)
        w.writerow(['core', 'id', 'group', 'op', 'passed', 'comparable', 'digest', 'detail'])
        for core, doc in (('go', go_doc), ('rust', rust_doc)):
            for c in doc['checks']:
                w.writerow([core, c['id'], c['group'], c['op'], c['passed'],
                            c.get('comparable', False), c.get('digest', ''), c.get('detail', '')])

    with open(os.path.join(out, 'counters.csv'), 'w', newline='') as fh:
        w = csv.writer(fh)
        w.writerow(['core', 'op', 'workload', 'rep', 'name', 'value'])
        for core, doc in (('go', go_doc), ('rust', rust_doc)):
            for c in doc['counters']:
                w.writerow([core, c['op'], c['workload'], c['rep'], c['name'], c['value']])

    with open(os.path.join(out, 'unsupported.csv'), 'w', newline='') as fh:
        w = csv.writer(fh)
        w.writerow(['core', 'op', 'workload', 'reason'])
        for core, doc in (('go', go_doc), ('rust', rust_doc)):
            for u in doc['unsupported']:
                w.writerow([core, u['op'], u.get('workload', ''), u['reason']])


def chart_spec(group, rows, identity, profile):
    categories, cells = [], []
    for r in rows:
        thr = 'auto' if r['threads'] == 0 else f'{r["threads"]}t'
        categories.append(f'{r["op"].split(".", 1)[1]} — {r["workload"]} [{thr}]')
        cells.append([
            {'label': fmt_ns(r['go']['time']['median']), 'value': r['go']['time']['median'],
             'high': r['go']['time']['p75'], 'n': r['go']['time']['n']},
            {'label': fmt_ns(r['rust']['time']['median']), 'value': r['rust']['time']['median'],
             'high': r['rust']['time']['p75'], 'n': r['rust']['time']['n']},
        ])
    return {
        'title': f'{group}: time per operation ({profile["name"]} profile)',
        'subtitle': [
            'Median of the per-repetition values, bar tip to the 75th percentile; '
            'lower is faster. Native library calls, measured in process.',
            f'Go {identity["cores"]["go"]["revision"][:12]} vs '
            f'Rust {identity["cores"]["rust"]["revision"][:12]}, '
            f'{profile["reps"]} repetitions, seed {profile["seed"]}.',
        ],
        'footer': [
            'No overall ranking is implied: the two cores trade differently across the '
            'operation set, and these bars are per operation only.',
            f'Host {identity["environment"]["host"]}, CPUs {identity["environment"]["cpu_set"]}, '
            f'warm caches.',
        ],
        'categories': categories,
        'series': ['Go core', 'Rust core'],
        'cells': cells,
    }


def render_plots(out, identity, profile, paired):
    try:
        import amber_bench_plot
    except ImportError:
        return []
    plot_dir = os.path.join(out, 'plots')
    os.makedirs(plot_dir, exist_ok=True)
    names = []
    for g in sorted({r['group'] for r in paired}):
        rows = sorted([r for r in paired if r['group'] == g],
                      key=lambda r: (r['op'], r['workload']))
        spec = chart_spec(g, rows, identity, profile)
        with open(os.path.join(plot_dir, f'{g}.json'), 'w') as fh:
            json.dump(spec, fh, indent=2, sort_keys=True)
        with open(os.path.join(plot_dir, f'{g}.svg'), 'w') as fh:
            fh.write(amber_bench_plot.render_svg(spec))
        names.append(g)
    return names


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--identity', required=True)
    ap.add_argument('--go', required=True)
    ap.add_argument('--rust', required=True)
    ap.add_argument('--out', required=True)
    ap.add_argument('--no-plots', action='store_true')
    args = ap.parse_args()

    identity = load(args.identity)
    go_doc, rust_doc = load(args.go), load(args.rust)

    try:
        validity = validate(identity, go_doc, rust_doc)
    except Invalid as e:
        print(f'core-ops report: refusing to publish a comparison: {e}', file=sys.stderr)
        sys.exit(1)

    go_cases, rust_cases = collect(go_doc), collect(rust_doc)
    go_sum, rust_sum = summarize(go_cases), summarize(rust_cases)
    paired, unpaired = pair(go_cases, rust_cases, go_sum, rust_sum)

    os.makedirs(args.out, exist_ok=True)
    write_csvs(args.out, go_doc, rust_doc, paired, unpaired, go_sum, rust_sum)

    plots = [] if args.no_plots else render_plots(args.out, identity, go_doc['config'], paired)

    report = {
        'schema': REPORT_SCHEMA,
        'profile': go_doc['config'],
        'identity': identity,
        'validity': validity,
        'paired': paired,
        'unpaired': unpaired,
        'unsupported': {'go': go_doc['unsupported'], 'rust': rust_doc['unsupported']},
        'blackhole': {'go': go_doc['blackhole'], 'rust': rust_doc['blackhole']},
    }
    with open(os.path.join(args.out, 'report.json'), 'w') as fh:
        json.dump(report, fh, indent=2, sort_keys=True, default=str)
    with open(os.path.join(args.out, 'REPORT.md'), 'w') as fh:
        fh.write(markdown(identity, go_doc, rust_doc, validity, paired, unpaired,
                          go_doc, rust_doc, plots))

    print(f'core-ops report: {len(paired)} paired cases, {len(unpaired)} single-core cases, '
          f'{len(plots)} charts -> {args.out}')


if __name__ == '__main__':
    main()
