"""What may be compared, and what must stop the report from existing.

`report.py` measures nothing and decides nothing about performance until
this module has agreed that the two documents describe the same experiment.
Every rule here exists because a run that broke it would still have produced
a plausible-looking table.

The rules are deliberately stricter than "both files parse". Independent
probes of an earlier version got a published comparison out of empty check
lists, out of two documents whose samples shared a label while one counted
one operation over 64 bytes and the other a hundred over 4096, and out of a
shared case that existed on one side only and quietly became an "operation
one core does not export". None of those survive this module.
"""
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

SAMPLE_SCHEMA = 'amber-core-ops/samples/1'
IDENTITY_SCHEMAS = ('amber-core-ops/identity/1', 'amber-core-ops/identity/2')

# The dimensions and counts that describe *what was asked of the core*. Two
# samples that share an (op, workload) label must agree on every one of them,
# because a label is not a measurement: the numbers below are.
CASE_FACTS = ('threads', 'ops', 'bytes', 'bytes_kind', 'dims')

# Fields of the environment block that must agree. A run where one core saw
# extended attributes and the other did not, or where the two got different
# automatic parallelism, is not a paired run.
ENVIRONMENT_FACTS = ('host', 'os', 'arch', 'auto_parallelism', 'scratch_fs', 'xattrs')


class Invalid(Exception):
    """A reason the two runs must not be compared."""


def _key(s):
    return (s['group'], s['op'], s['workload'], s['threads'])


def _finite(x):
    return isinstance(x, (int, float)) and not isinstance(x, bool) and math.isfinite(x)


def merge_passes(docs, core):
    """Merges the per-pass documents of one core into a single document.

    run.sh measures a profile in two passes of opposite core order, so
    neither core is systematically measured on the colder machine. Each pass
    writes its own document covering a slice of the repetitions; the report
    reads the union. Everything that is a property of the run rather than of
    a pass -- the configuration, the identity, the environment, the checks,
    the encodings, the wire inputs -- must be identical in every pass, and a
    repetition index may not appear twice.
    """
    if not docs:
        raise Invalid(f'no {core} sample document was given')
    head = docs[0]
    for i, doc in enumerate(docs):
        if doc.get('schema') != SAMPLE_SCHEMA:
            raise Invalid(
                f'{core} pass {i} has schema {doc.get("schema")!r}, expected {SAMPLE_SCHEMA!r}')
        if doc.get('core') != core:
            raise Invalid(f'{core} pass {i} declares core {doc.get("core")!r}')
        for field in ('config', 'identity', 'environment'):
            actual, expected = doc.get(field), head.get(field)
            if field == 'environment':
                # Each pass owns a fresh scratch directory on the same filesystem.
                actual = {k: v for k, v in (actual or {}).items() if k != 'scratch'}
                expected = {k: v for k, v in (expected or {}).items() if k != 'scratch'}
            if actual != expected:
                raise Invalid(f'{core} pass {i} disagrees with pass 0 on {field}')
        def check_facts(document):
            # Local diagnostic counts can vary with segment layout. Every pass
            # must still pass the same checks and match comparable digests.
            checks = document.get('checks') or []
            if not checks:
                raise Invalid(f'the {core} run recorded no correctness checks at all')
            facts = {}
            for check in checks:
                if not check.get('passed'):
                    raise Invalid(f'{core} run has failing checks: {check["id"]}')
                if check['id'] in facts:
                    raise Invalid(f'{core} recorded check id {check["id"]} more than once')
                facts[check['id']] = {
                    k: v for k, v in check.items()
                    if k != 'digest' or check.get('comparable')
                }
            return facts

        if check_facts(doc) != check_facts(head):
            raise Invalid(f'{core} pass {i} disagrees with pass 0 on checks')
        for field in ('unsupported', 'encodings', 'wire_inputs'):
            if doc.get(field) != head.get(field):
                raise Invalid(f'{core} pass {i} disagrees with pass 0 on {field}')

    merged = dict(head)
    samples = []
    seen = set()
    for i, doc in enumerate(docs):
        for s in doc['samples']:
            ident = (_key(s), s['rep'])
            if ident in seen:
                raise Invalid(
                    f'{core} measures {s["op"]}/{s["workload"]} repetition {s["rep"]} twice; '
                    f'the passes overlap')
            seen.add(ident)
            samples.append(s)
    merged['samples'] = samples
    merged['counters'] = [c for doc in docs for c in doc['counters']]
    merged['blackhole'] = sum(doc['blackhole'] for doc in docs)
    merged['passes'] = len(docs)
    merged['checks_by_pass'] = [doc['checks'] for doc in docs]
    return merged


def _check_identity(identity, go, rust, ok):
    if identity.get('schema') not in IDENTITY_SCHEMAS:
        raise Invalid(f'the identity document has schema {identity.get("schema")!r}')
    cores = identity.get('cores') or {}
    for name, doc in (('go', go), ('rust', rust)):
        if name not in cores:
            raise Invalid(f'the identity document does not describe the {name} core')
        rev = cores[name].get('revision')
        if not (isinstance(rev, str) and len(rev) == 40 and
                all(c in '0123456789abcdef' for c in rev)):
            raise Invalid(f'{name} core has no recorded 40-character revision')
        sha = cores[name].get('driver_sha256')
        if not (isinstance(sha, str) and len(sha) == 64):
            raise Invalid(f'{name} driver has no recorded 64-character executable hash')
        # The raw document has to be the one this identity describes. Without
        # this, an identity file could name any revision at all and the
        # samples beside it would never contradict it.
        did = doc.get('identity') or {}
        for field, want in (('core_revision', rev),
                            ('driver_sha256', sha),
                            ('core_dirty', cores[name].get('dirty'))):
            if did.get(field) != want:
                raise Invalid(
                    f'the {name} samples record {field}={did.get(field)!r}, but the identity '
                    f'document records {want!r}')
    ok.append('both cores are identified by a full commit id and an executable hash, '
              'and each raw document carries the same identity')


def _check_config(go, rust, ok):
    gc_, rc = go.get('config') or {}, rust.get('config') or {}
    if not gc_ or not rc:
        raise Invalid('a run has no configuration block')
    fields = sorted(set(gc_) | set(rc))
    for field in fields:
        if gc_.get(field) != rc.get(field):
            raise Invalid(
                f'the two runs disagree on config.{field}: {gc_.get(field)!r} vs {rc.get(field)!r}')
    ok.append(f'both runs used the same profile and all {len(fields)} configuration fields')


def _check_environment(go, rust, ok):
    ge, re_ = go.get('environment') or {}, rust.get('environment') or {}
    for field in ENVIRONMENT_FACTS:
        if field not in ge or field not in re_:
            raise Invalid(f'a run does not record environment.{field}')
        if ge[field] != re_[field]:
            raise Invalid(
                f'the two runs disagree on environment.{field}: {ge[field]!r} vs {re_[field]!r}')
    ok.append('both runs saw the same host, filesystem, extended-attribute support '
              f'and automatic parallelism ({ge["auto_parallelism"]})')


def _check_checks(go, rust, ok):
    for name, doc in (('go', go), ('rust', rust)):
        checks = doc.get('checks')
        if not checks:
            raise Invalid(f'the {name} run recorded no correctness checks at all')
        ids = [c['id'] for c in checks]
        dupes = sorted({i for i in ids if ids.count(i) > 1})
        if dupes:
            raise Invalid(f'{name} records the same check id more than once: {dupes[:5]}')
        failed = sorted(c['id'] for c in checks if not c['passed'])
        if failed:
            raise Invalid(f'{name} run has failing checks: {", ".join(failed)}')
    ok.append(f'every correctness check passed ({len(go["checks"])} Go, {len(rust["checks"])} Rust), '
              'and no check id is recorded twice')

    gcomp = {c['id']: c['digest'] for c in go['checks'] if c.get('comparable')}
    rcomp = {c['id']: c['digest'] for c in rust['checks'] if c.get('comparable')}
    if not gcomp:
        raise Invalid('no check is marked comparable, so nothing was proved about agreement')
    only_go = sorted(set(gcomp) - set(rcomp))
    only_rust = sorted(set(rcomp) - set(gcomp))
    if only_go or only_rust:
        raise Invalid(
            'a check marked comparable exists in one core only: '
            f'go-only={only_go} rust-only={only_rust}')
    empty = sorted(k for k, v in gcomp.items() if not v)
    if empty:
        raise Invalid(f'a comparable check recorded no digest: {empty[:5]}')
    mismatched = sorted(k for k in gcomp if gcomp[k] != rcomp[k])
    if mismatched:
        raise Invalid('cross-core digests differ: ' + ', '.join(mismatched))
    ok.append(f'all {len(gcomp)} cross-core output digests are identical')


def _check_samples(go, rust, reps, ok):
    for name, doc in (('go', go), ('rust', rust)):
        if not doc.get('samples'):
            raise Invalid(f'the {name} run recorded no samples at all')
        for s in doc['samples']:
            label = f'{name} sample {s["op"]}/{s["workload"]}'
            if s['status'] != 'ok':
                raise Invalid(f'{label} has status {s["status"]}')
            for field in ('wall_ns', 'cpu_user_ns', 'cpu_sys_ns', 'ops', 'bytes', 'rep'):
                if not _finite(s.get(field)):
                    raise Invalid(f'{label} has a non-finite {field}: {s.get(field)!r}')
            if s['wall_ns'] <= 0 or s['ops'] <= 0:
                raise Invalid(f'{label} has a non-positive time or operation count')
            if s['bytes'] < 0:
                raise Invalid(f'{label} has a negative byte count')
            if s['bytes'] > 0 and not s.get('bytes_kind'):
                raise Invalid(f'{label} reports bytes without saying what they count')
    ok.append('every sample records a finite, positive elapsed time and operation count, '
              'and every byte count says what it counts')

    # Exactly the profile's repetitions, each index once, for every case.
    for name, doc in (('go', go), ('rust', rust)):
        by_case = {}
        for s in doc['samples']:
            by_case.setdefault(_key(s), []).append(s['rep'])
        bad = []
        for k, got in sorted(by_case.items()):
            if sorted(got) != list(range(reps)):
                bad.append((k, sorted(got)))
        if bad:
            raise Invalid(
                f'{name} does not have exactly repetitions 0..{reps - 1} of every case: '
                f'{bad[:3]}')
    ok.append(f'every case has exactly repetitions 0..{reps - 1} on both sides, each once')

    # Within a core, every repetition of a case must describe the same work.
    for name, doc in (('go', go), ('rust', rust)):
        first = {}
        for s in doc['samples']:
            k = _key(s)
            facts = tuple(s.get(f) for f in CASE_FACTS)
            if k not in first:
                first[k] = facts
            elif first[k] != facts:
                raise Invalid(
                    f'{name} changes the dimensions of {k[1]}/{k[2]} between repetitions: '
                    f'{first[k]} vs {facts}')
    ok.append('every repetition of a case describes the same work as its siblings')


def _check_case_agreement(go, rust, ok):
    """The heart of it: a shared label must mean a shared experiment."""
    gcases, rcases = {}, {}
    for doc, into in ((go, gcases), (rust, rcases)):
        for s in doc['samples']:
            into.setdefault(_key(s), s)

    shared = sorted(set(gcases) & set(rcases))
    if not shared:
        raise Invalid('the two runs share no case at all')
    for k in shared:
        g, r = gcases[k], rcases[k]
        for field in CASE_FACTS:
            if g.get(field) != r.get(field):
                raise Invalid(
                    f'{k[1]}/{k[2]} is not the same experiment in the two cores: '
                    f'{field} is {g.get(field)!r} in Go and {r.get(field)!r} in Rust')
    ok.append(f'all {len(shared)} shared cases agree on threads, operation count, '
              'byte count and every workload dimension')

    # A shared label that exists on one side only is a broken pair, not an
    # operation one core does not export. The difference matters: an absent
    # operation must have been declared absent, with a reason, by the core
    # that lacks it.
    for name, mine, theirs in (('go', gcases, rcases), ('rust', rcases, gcases)):
        other = rust if name == 'go' else go
        declared = {u['op'] for u in other['unsupported']}
        their_ops = {k[1] for k in theirs}
        for k in sorted(set(mine) - set(theirs)):
            op, workload = k[1], k[2]
            if op in their_ops:
                raise Invalid(
                    f'{op}/{workload} was measured by {name} only, but the other core measures '
                    f'{op} at other workloads; a missing shared case is a broken pair, not an '
                    f'operation one core does not export')
            if op not in declared:
                raise Invalid(
                    f'{op} is measured by {name} only and the other core does not declare it '
                    f'unsupported; an absent operation must be declared, not inferred')
    ok.append('a case measured by one core only belongs to an operation the other core '
              'explicitly declares unsupported')

    for name, doc in (('go', go), ('rust', rust)):
        unsupported = {u['op'] for u in doc['unsupported']}
        measured = {s['op'] for s in doc['samples']}
        overlap = sorted(unsupported & measured)
        if overlap:
            raise Invalid(f'{name} both declares and measures: {", ".join(overlap)}')
        for u in doc['unsupported']:
            if not u.get('reason'):
                raise Invalid(f'{name} declares {u["op"]} unsupported without a reason')
    ok.append('no core both declares an operation unsupported and measures it, '
              'and every declaration carries a reason')
    return gcases, rcases


def _check_checksums(go, rust, ok):
    """The measured work really happened, and produced the same output."""
    stable_cases = 0
    for name, doc in (('go', go), ('rust', rust)):
        by_case = {}
        for s in doc['samples']:
            by_case.setdefault(_key(s), []).append(s)
        for k, ss in sorted(by_case.items()):
            if not ss[0].get('stable', True):
                continue
            sums = {s['checksum'] for s in ss}
            if len(sums) != 1:
                raise Invalid(
                    f'{name} {k[1]}/{k[2]} is declared stable but produced {len(sums)} '
                    f'different outputs across its repetitions')
            stable_cases += 1
    ok.append(f'every repetition of each of the {stable_cases // 2} stable cases produced '
              'the same output')

    gall = {_key(s) for s in go['samples']}
    rall = {_key(s) for s in rust['samples']}
    shared = gall & rall
    gx = {_key(s): s for s in go['samples'] if s.get('cross_checksum')}
    rx = {_key(s): s for s in rust['samples'] if s.get('cross_checksum')}
    # A case only one core measures cannot require the two to agree; claiming
    # it would be a contradiction rather than a stricter check.
    lonely = sorted((set(gx) | set(rx)) - shared)
    if lonely:
        raise Invalid('a single-core case claims cross-core output equality: '
                      f'{[f"{k[1]}/{k[2]}" for k in lonely[:3]]}')
    gx = {k: v for k, v in gx.items() if k in shared}
    rx = {k: v for k, v in rx.items() if k in shared}
    if not gx:
        raise Invalid('no case requires the two cores to produce the same output')
    only_go = sorted(set(gx) - set(rx))
    only_rust = sorted(set(rx) - set(gx))
    if only_go or only_rust:
        raise Invalid('a case requires cross-core output equality on one side only: '
                      f'go-only={only_go[:3]} rust-only={only_rust[:3]}')
    bad = sorted(k for k in gx if gx[k]['checksum'] != rx[k]['checksum'])
    if bad:
        raise Invalid('the two cores produced different output for: '
                      + ', '.join(f'{k[1]}/{k[2]}' for k in bad[:5]))
    ok.append(f'the two cores produced byte-identical output in all {len(gx)} cases that '
              'require it')


def _check_wire_inputs(go, rust, ok):
    """A decode comparison has to have decoded the same bytes."""
    gw = {w['producer']: w for w in go.get('wire_inputs', [])}
    rw = {w['producer']: w for w in rust.get('wire_inputs', [])}
    decoders = {s['workload'].rsplit('/producer-', 1)[1]
                for s in go['samples'] if '/producer-' in s['workload']}
    if decoders and (not gw or not rw):
        raise Invalid('a per-producer decode case was measured, but a run recorded no '
                      'wire inputs to say which bytes it decoded')
    if set(gw) != set(rw):
        raise Invalid(f'the two runs read different wire inputs: {sorted(gw)} vs {sorted(rw)}')
    for producer in sorted(gw):
        if gw[producer]['sha256'] != rw[producer]['sha256']:
            raise Invalid(
                f'the two runs read different bytes for the {producer} wire pack: '
                f'{gw[producer]["sha256"]} vs {rw[producer]["sha256"]}')
        if gw[producer]['bytes'] != rw[producer]['bytes']:
            raise Invalid(f'the {producer} wire pack has two different sizes')
    missing = sorted(decoders - set(gw))
    if missing:
        raise Invalid(f'a decode case names producers with no recorded wire input: {missing}')
    if gw:
        ok.append(f'both cores decoded the same bytes: {len(gw)} wire inputs, '
                  'identical content hashes')


def _check_encodings(go, rust, ok):
    """Encoders may differ in size; they may not differ in what they were fed."""
    ge = {(e['op'], e['workload']): e for e in go.get('encodings', [])}
    re_ = {(e['op'], e['workload']): e for e in rust.get('encodings', [])}
    if set(ge) != set(re_):
        raise Invalid('the two runs record encoded sizes for different cases')
    for k in sorted(ge):
        for field in ('logical_bytes', 'items'):
            if ge[k][field] != re_[k][field]:
                raise Invalid(
                    f'the two cores encoded different input for {k[0]}/{k[1]}: '
                    f'{field} {ge[k][field]} vs {re_[k][field]}')
    if ge:
        ok.append(f'the two cores encoded the same input in all {len(ge)} recorded encodings; '
                  'their output sizes are reported separately and never divided')


def _check_manifest(gcases, rcases, go, rust, manifest, ok):
    """The measured set is the set the coverage manifest says it should be."""
    if manifest is None:
        ok.append('WARNING: no coverage manifest was given, so the measured set was not '
                  'checked against one')
        return
    expected = manifest.get('operations') or {}
    if not expected:
        raise Invalid('the coverage manifest defines no operations')

    paired = {}
    for k in set(gcases) & set(rcases):
        paired.setdefault(k[1], set()).add(k[2])
    missing_ops = sorted(set(expected) - set(paired))
    if missing_ops:
        raise Invalid(f'the manifest expects paired samples for operations that have none: '
                      f'{missing_ops}')
    extra_ops = sorted(set(paired) - set(expected))
    if extra_ops:
        raise Invalid(f'operations were paired that the manifest does not list: {extra_ops}')
    for op in sorted(expected):
        want, got = set(expected[op]), paired[op]
        if want != got:
            raise Invalid(
                f'{op} was measured at workloads the manifest does not expect: '
                f'missing={sorted(want - got)} unexpected={sorted(got - want)}')

    for core, doc, cases in (('go', go, gcases), ('rust', rust, rcases)):
        want = set(tuple(x) for x in (manifest.get('single_core') or {}).get(core, []))
        got = {(k[1], k[2]) for k in cases} - {(k[1], k[2]) for k in
                                               (set(gcases) & set(rcases))}
        if want != got:
            raise Invalid(
                f'the {core}-only measured set is not the manifest\'s: '
                f'missing={sorted(want - got)} unexpected={sorted(got - want)}')

    # A check the manifest cites has to have been recorded by the core that
    # can run it. Some are single-core by construction -- a Rust-only module
    # has Rust-only evidence -- so the requirement is "at least one side",
    # and the cross-core equality of the comparable ones is a separate rule.
    want_checks = set(manifest.get('checks') or [])
    if want_checks:
        got = {c['id'] for c in go['checks']} | {c['id'] for c in rust['checks']}
        missing = sorted(want_checks - got)
        if missing:
            raise Invalid(f'the coverage manifest cites checks that no run recorded: '
                          f'{missing}')
    n_cases = sum(len(v) for v in expected.values())
    ok.append(f'the measured set is exactly the manifest\'s: {len(expected)} paired '
              f'operations over {n_cases} workloads, and every check it names was recorded')


def validate(identity, go, rust, manifest=None):
    """Returns the list of validity statements, raising on the first failure."""
    ok = []
    ok.append('both runs carry the shared raw-sample schema')
    _check_config(go, rust, ok)
    _check_identity(identity, go, rust, ok)
    _check_environment(go, rust, ok)
    _check_checks(go, rust, ok)
    _check_samples(go, rust, go['config']['reps'], ok)
    gcases, rcases = _check_case_agreement(go, rust, ok)
    _check_checksums(go, rust, ok)
    _check_wire_inputs(go, rust, ok)
    _check_encodings(go, rust, ok)
    _check_manifest(gcases, rcases, go, rust, manifest, ok)

    passes = go.get('passes', 1)
    if passes > 1:
        ok.append(f'the repetitions were measured in {passes} passes of opposite core order, '
                  'so neither core was systematically measured first')
    if not identity.get('pin_verified'):
        ok.append('WARNING: the run did not verify against the pinned core revisions')
    return ok


def main(argv):  # pragma: no cover - a convenience for shell use
    import json
    if len(argv) < 4:
        print('usage: validate.py identity.json go.json rust.json [cases.json]', file=sys.stderr)
        return 2
    with open(argv[1]) as fh:
        identity = json.load(fh)
    go = merge_passes([json.load(open(argv[2]))], 'go')
    rust = merge_passes([json.load(open(argv[3]))], 'rust')
    manifest = json.load(open(argv[4])) if len(argv) > 4 else None
    try:
        for line in validate(identity, go, rust, manifest):
            print(line)
    except Invalid as e:
        print(f'invalid: {e}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':  # pragma: no cover
    sys.exit(main(sys.argv))
