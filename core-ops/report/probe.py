#!/usr/bin/env python3
"""Negative probes: take a real, valid pair of documents and break one thing.

`test_report.py` tests the gate against documents this file wrote. This one
tests it against documents the *drivers* wrote -- a whole run, with its real
checks, real dimensions and real wire inputs -- and then breaks exactly one
thing about it. A rule that only refuses hand-made minimal documents is a
rule that has not met the shape of a real run.

    python3 core-ops/report/probe.py <results-dir>/<profile-dir>

The directory is one profile's output: the `identity.json` beside it and the
`go-pass*.json` / `rust-pass*.json` in it.
"""
import copy
import glob
import json
import os
import subprocess
import sys
import tempfile

if len(sys.argv) != 2:
    print(__doc__.strip(), file=sys.stderr)
    raise SystemExit(2)

PROF = os.path.abspath(sys.argv[1])
HERE = os.path.dirname(os.path.abspath(__file__))
REPORT = os.path.join(HERE, 'report.py')
ID = json.load(open(os.path.join(os.path.dirname(PROF), 'identity.json')))
GO = json.load(open(sorted(glob.glob(f'{PROF}/go-pass*.json'))[0]))
RS = json.load(open(sorted(glob.glob(f'{PROF}/rust-pass*.json'))[0]))

def probe(name, mutate):
    g, r, i = copy.deepcopy(GO), copy.deepcopy(RS), copy.deepcopy(ID)
    mutate(g, r, i)
    with tempfile.TemporaryDirectory() as tmp:
        for n, d in (('identity', i), ('go', g), ('rust', r)):
            json.dump(d, open(f'{tmp}/{n}.json', 'w'))
        out = f'{tmp}/out'
        p = subprocess.run([sys.executable, REPORT,
                            '--identity', f'{tmp}/identity.json', '--go', f'{tmp}/go.json',
                            '--rust', f'{tmp}/rust.json', '--out', out, '--no-plots'],
                           capture_output=True, text=True)
        wrote = os.path.exists(f'{out}/REPORT.md')
        status = 'REFUSED' if p.returncode != 0 and not wrote else 'ACCEPTED'
        reason = p.stderr.strip().split(': ', 2)[-1][:110] if p.returncode else ''
        print(f'  {status:9} {name}\n            {reason}')
        return status == 'REFUSED'

def empty_checks(g, r, i): g['checks'] = []
def empty_samples(g, r, i): g['samples'] = []
def mismatched_labels(g, r, i):
    for s in g['samples']:
        if s['op'] == 'key.new' and s['workload'] == '4KiB-random':
            s['ops'], s['bytes'] = 1, 64
    for s in r['samples']:
        if s['op'] == 'key.new' and s['workload'] == '4KiB-random':
            s['ops'], s['bytes'] = 100, 4096
def drop_a_shared_case(g, r, i):
    r['samples'] = [s for s in r['samples']
                    if not (s['op'] == 'key.new' and s['workload'] == '4KiB-text')]
def flip_a_digest(g, r, i):
    for c in r['checks']:
        if c['id'] == 'ingest/objects-root':
            c['digest'] = '0' * 64
def fail_a_check(g, r, i):
    g['checks'][5]['passed'] = False
def wrong_revision(g, r, i):
    i['cores']['go']['revision'] = '9' * 40
def wrong_driver_hash(g, r, i):
    i['cores']['rust']['driver_sha256'] = '0' * 64
def different_wire_input(g, r, i):
    r['wire_inputs'][0]['sha256'] = '0' * 64
def different_encoder_input(g, r, i):
    r['encodings'][0]['logical_bytes'] += 1
def different_parallelism(g, r, i):
    r['environment']['auto_parallelism'] = 4
def different_output(g, r, i):
    for s in r['samples']:
        if s['op'] == 'ingest.objects':
            s['checksum'] = 1
def extra_unlisted_workload(g, r, i):
    for d in (g, r):
        extra = copy.deepcopy([s for s in d['samples'] if s['op'] == 'key.new'][0])
        extra['workload'] = 'invented'
        d['samples'].append(extra)
def negative_time(g, r, i):
    g['samples'][0]['wall_ns'] = -1
def undeclared_single_core(g, r, i):
    r['unsupported'] = []

probes = [
    ('an empty check list', empty_checks),
    ('an empty sample list', empty_samples),
    ('the same label over ops=1/bytes=64 vs ops=100/bytes=4096', mismatched_labels),
    ('a shared case present on one side only', drop_a_shared_case),
    ('a cross-core digest flipped', flip_a_digest),
    ('one failing check', fail_a_check),
    ('an identity naming a revision the samples do not', wrong_revision),
    ('an identity naming another executable', wrong_driver_hash),
    ('the two cores decoding different wire bytes', different_wire_input),
    ('the two encoders fed different input', different_encoder_input),
    ('different automatic parallelism', different_parallelism),
    ('the two cores producing different output', different_output),
    ('a workload the coverage matrix does not list', extra_unlisted_workload),
    ('a negative elapsed time', negative_time),
    ('an undeclared single-core operation', undeclared_single_core),
]
print(f'{len(probes)} negative probes against a real, valid pair of documents:')
ok = sum(probe(n, f) for n, f in probes)
print(f'\n{ok}/{len(probes)} refused')
sys.exit(0 if ok == len(probes) else 1)
