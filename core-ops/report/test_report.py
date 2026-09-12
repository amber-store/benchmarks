"""Tests for the core-operation report: the statistics and the validity gate.

The gate is the part worth testing hardest. Every rule in it exists because
publishing a comparison that has not earned it is worse than publishing
nothing, so each rule gets a test that proves it actually refuses.
"""
import json
import math
import os
import sys
import tempfile
import unittest
from math import comb

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import report  # noqa: E402
import stats  # noqa: E402


class TestDescriptives(unittest.TestCase):
    def test_median_odd_and_even(self):
        self.assertEqual(stats.median([3, 1, 2]), 2)
        self.assertEqual(stats.median([4, 1, 3, 2]), 2.5)

    def test_median_rejects_empty(self):
        with self.assertRaises(ValueError):
            stats.median([])

    def test_quantile_matches_linear_interpolation(self):
        xs = [1, 2, 3, 4]
        self.assertEqual(stats.quantile(xs, 0.0), 1.0)
        self.assertEqual(stats.quantile(xs, 1.0), 4.0)
        self.assertAlmostEqual(stats.quantile(xs, 0.25), 1.75)
        self.assertAlmostEqual(stats.quantile(xs, 0.5), 2.5)

    def test_quantile_of_single_value(self):
        self.assertEqual(stats.quantile([7], 0.3), 7.0)

    def test_mad_ignores_one_outlier(self):
        base = [10, 10, 10, 10, 10]
        self.assertEqual(stats.mad(base), 0)
        self.assertEqual(stats.mad(base + [1000]), 0)

    def test_describe_reports_every_field(self):
        d = stats.describe([5, 1, 3, 2, 4])
        self.assertEqual(d['n'], 5)
        self.assertEqual(d['min'], 1)
        self.assertEqual(d['max'], 5)
        self.assertEqual(d['median'], 3)
        self.assertEqual(d['mean'], 3)


class TestExactMannWhitney(unittest.TestCase):
    def test_distribution_is_complete_and_symmetric(self):
        for n in range(1, 9):
            for m in range(1, 9):
                counts = stats._exact_u_counts(n, m)
                self.assertEqual(sum(counts), comb(n + m, n), f'n={n} m={m}')
                self.assertEqual(counts, counts[::-1], f'n={n} m={m}')

    def test_fully_separated_samples_give_the_extreme_p(self):
        a = [1, 2, 3, 4, 5, 6, 7]
        b = [10, 11, 12, 13, 14, 15, 16]
        u, p, method = stats.mann_whitney_u(a, b)
        self.assertEqual(method, 'exact')
        self.assertEqual(u, 0.0)
        # Both tails of a 7-vs-7 arrangement: 2 of C(14,7).
        self.assertAlmostEqual(p, 2 / comb(14, 7))

    def test_identical_samples_are_not_significant(self):
        _, p, _ = stats.mann_whitney_u([1, 2, 3], [1, 2, 3])
        self.assertEqual(p, 1.0)

    def test_ties_fall_back_to_the_corrected_normal_approximation(self):
        a = [1, 1, 1, 2, 2, 2]
        b = [1, 1, 2, 2, 3, 3]
        _, p, method = stats.mann_whitney_u(a, b)
        self.assertEqual(method, 'normal')
        self.assertTrue(0.0 <= p <= 1.0)

    def test_empty_sample_is_an_error(self):
        with self.assertRaises(ValueError):
            stats.mann_whitney_u([], [1])


class TestBootstrap(unittest.TestCase):
    def test_interval_brackets_the_point_estimate(self):
        a = [10, 11, 12, 10, 11, 12, 11]
        b = [5, 5, 6, 5, 6, 5, 5]
        lo, hi = stats.bootstrap_ratio_ci(a, b)
        point = stats.median(a) / stats.median(b)
        self.assertLessEqual(lo, point)
        self.assertGreaterEqual(hi, point)

    def test_is_reproducible(self):
        a, b = [3, 4, 5, 6, 7], [1, 2, 3, 4, 5]
        self.assertEqual(stats.bootstrap_ratio_ci(a, b), stats.bootstrap_ratio_ci(a, b))

    def test_identical_samples_give_an_interval_around_one(self):
        xs = [7, 7, 7, 7, 7]
        lo, hi = stats.bootstrap_ratio_ci(xs, xs)
        self.assertEqual((lo, hi), (1.0, 1.0))


def sample(op='key.new', workload='w', core='go', rep=0, wall=1000):
    return {
        'group': op.split('.')[0], 'op': op, 'workload': workload, 'threads': 1,
        'rep': rep, 'ops': 10, 'bytes': 100, 'wall_ns': wall,
        'cpu_user_ns': wall, 'cpu_sys_ns': 0, 'max_rss_kib': 1000, 'status': 'ok',
    }


def doc(core, **over):
    base = {
        'schema': report.SCHEMA,
        'core': core,
        'profile': 'standard',
        'config': {f: v for f, v in [
            ('name', 'standard'), ('seed', 1), ('reps', 2), ('warmup', 0),
            ('threads_single', 1), ('threads_multi', 8), ('corpus_bytes', 1),
            ('tree_files', 1), ('tree_wide', 1), ('tree_depth', 1),
            ('synthetic_wide', 1), ('store_objects', 1), ('segment_bytes', 1),
            ('ref_records', 1), ('inbox_packs', 1), ('batch_ops', 1)]},
        'identity': {'toolchain': 'test'},
        'environment': {'xattrs': False},
        'started_at': '', 'finished_at': '',
        'checks': [{'id': 'c/1', 'group': 'key', 'op': 'key.new', 'passed': True,
                    'detail': '', 'digest': 'abc', 'comparable': True}],
        'samples': [sample(rep=0, wall=1000 if core == 'go' else 500),
                    sample(rep=1, wall=1100 if core == 'go' else 550)],
        'counters': [],
        'unsupported': [],
        'blackhole': 1,
    }
    base.update(over)
    return base


def identity(**over):
    base = {
        'schema': 'amber-core-ops/identity/1',
        'pin_verified': True,
        'flake_lock': {'amber_go_src': 'a' * 40, 'amber_rust_src': 'b' * 40},
        'harness': {'revision': 'c' * 40, 'dirty': False, 'sources_sha256': 'd'},
        'cores': {
            'go': {'source': 'pinned', 'revision': 'a' * 40, 'dirty': False,
                   'module': 'm', 'module_tree_vs_pinned_source': 'identical',
                   'driver': 'x', 'driver_sha256': 'e' * 64},
            'rust': {'source': 'pinned', 'revision': 'b' * 40, 'dirty': False,
                     'module': 'm', 'cargo_lock_rev': 'b' * 40,
                     'driver': 'y', 'driver_sha256': 'f' * 64},
        },
        'environment': {'host': 'h', 'kernel': 'k', 'cpu_model': 'c',
                        'cpu_count': 8, 'cpu_set': '0-7', 'mem_total_kb': 1024,
                        'scratch': '/s', 'scratch_fs': 'ext4'},
    }
    base.update(over)
    return base


class TestValidity(unittest.TestCase):
    def test_a_clean_pair_validates(self):
        ok = report.validate(identity(), doc('go'), doc('rust'))
        self.assertTrue(any('correctness check passed' in s for s in ok))
        self.assertTrue(any('digests are identical' in s for s in ok))

    def test_a_wrong_schema_is_refused(self):
        with self.assertRaisesRegex(report.Invalid, 'schema'):
            report.validate(identity(), doc('go', schema='other'), doc('rust'))

    def test_a_configuration_difference_is_refused(self):
        r = doc('rust')
        r['config']['seed'] = 99
        with self.assertRaisesRegex(report.Invalid, 'disagree on seed'):
            report.validate(identity(), doc('go'), r)

    def test_a_missing_revision_is_refused(self):
        ident = identity()
        ident['cores']['rust']['revision'] = 'short'
        with self.assertRaisesRegex(report.Invalid, 'revision'):
            report.validate(ident, doc('go'), doc('rust'))

    def test_a_missing_driver_hash_is_refused(self):
        ident = identity()
        ident['cores']['go']['driver_sha256'] = ''
        with self.assertRaisesRegex(report.Invalid, 'executable hash'):
            report.validate(ident, doc('go'), doc('rust'))

    def test_a_failed_check_is_refused(self):
        g = doc('go')
        g['checks'][0]['passed'] = False
        with self.assertRaisesRegex(report.Invalid, 'failing checks'):
            report.validate(identity(), g, doc('rust'))

    def test_a_cross_core_digest_mismatch_is_refused(self):
        r = doc('rust')
        r['checks'][0]['digest'] = 'different'
        with self.assertRaisesRegex(report.Invalid, 'digests differ'):
            report.validate(identity(), doc('go'), r)

    def test_a_comparable_check_on_one_side_only_is_refused(self):
        r = doc('rust')
        r['checks'] = []
        with self.assertRaisesRegex(report.Invalid, 'one core only'):
            report.validate(identity(), doc('go'), r)

    def test_measuring_something_declared_unsupported_is_refused(self):
        g = doc('go')
        g['unsupported'] = [{'op': 'key.new', 'workload': '', 'reason': 'r'}]
        with self.assertRaisesRegex(report.Invalid, 'declares and measures'):
            report.validate(identity(), g, doc('rust'))

    def test_a_zero_time_sample_is_refused(self):
        g = doc('go')
        g['samples'][0]['wall_ns'] = 0
        with self.assertRaisesRegex(report.Invalid, 'non-positive'):
            report.validate(identity(), g, doc('rust'))

    def test_a_failed_sample_status_is_refused(self):
        g = doc('go')
        g['samples'][0]['status'] = 'error'
        with self.assertRaisesRegex(report.Invalid, 'status'):
            report.validate(identity(), g, doc('rust'))

    def test_a_missing_repetition_is_refused(self):
        g = doc('go')
        g['samples'] = g['samples'][:1]
        with self.assertRaisesRegex(report.Invalid, 'repetitions'):
            report.validate(identity(), g, doc('rust'))

    def test_an_unverified_pin_is_reported_rather_than_hidden(self):
        ok = report.validate(identity(pin_verified=False), doc('go'), doc('rust'))
        self.assertTrue(any('WARNING' in s for s in ok))


class TestPairing(unittest.TestCase):
    def test_pairs_are_built_per_operation_and_workload(self):
        g, r = doc('go'), doc('rust')
        gc_, rc = report.collect(g), report.collect(r)
        paired, unpaired = report.pair(gc_, rc, report.summarize(gc_), report.summarize(rc))
        self.assertEqual(len(paired), 1)
        self.assertEqual(len(unpaired), 0)
        row = paired[0]
        # Go 1000/1100 ns over 10 ops, Rust 500/550: a ratio of exactly two.
        self.assertAlmostEqual(row['go']['time']['median'], 105.0)
        self.assertAlmostEqual(row['rust']['time']['median'], 52.5)
        self.assertAlmostEqual(row['median_ratio_go_over_rust'], 2.0)

    def test_an_operation_only_one_core_has_is_never_paired(self):
        g = doc('go')
        g['samples'] += [sample(op='gc.begin_write', rep=0), sample(op='gc.begin_write', rep=1)]
        gc_, rc = report.collect(g), report.collect(doc('rust'))
        paired, unpaired = report.pair(gc_, rc, report.summarize(gc_), report.summarize(rc))
        self.assertEqual(len(paired), 1)
        self.assertEqual([u['op'] for u in unpaired], ['gc.begin_write'])
        self.assertEqual(unpaired[0]['core'], 'go')

    def test_throughput_is_only_derived_where_bytes_are_meaningful(self):
        g = doc('go')
        for s in g['samples']:
            s['bytes'] = 0
        summary = report.summarize(report.collect(g))
        self.assertNotIn('throughput', list(summary.values())[0])


class TestOutput(unittest.TestCase):
    def test_a_full_run_writes_every_artefact(self):
        with tempfile.TemporaryDirectory() as tmp:
            paths = {}
            for name, payload in (('identity', identity()), ('go', doc('go')),
                                  ('rust', doc('rust'))):
                paths[name] = os.path.join(tmp, f'{name}.json')
                with open(paths[name], 'w') as fh:
                    json.dump(payload, fh)
            out = os.path.join(tmp, 'out')
            argv = sys.argv
            sys.argv = ['report.py', '--identity', paths['identity'], '--go', paths['go'],
                        '--rust', paths['rust'], '--out', out, '--no-plots']
            try:
                report.main()
            finally:
                sys.argv = argv
            for name in ('REPORT.md', 'report.json', 'samples.csv', 'summary.csv',
                         'paired.csv', 'checks.csv', 'counters.csv', 'unsupported.csv'):
                self.assertTrue(os.path.exists(os.path.join(out, name)), name)
            with open(os.path.join(out, 'REPORT.md')) as fh:
                text = fh.read()
            self.assertIn('No overall ranking', text.replace('no overall ranking',
                                                            'No overall ranking'))
            self.assertIn('a' * 40, text)  # the Go core revision is displayed
            self.assertIn('b' * 40, text)  # and the Rust one
            self.assertIn('c' * 40, text)  # and the harness revision, separately
            with open(os.path.join(out, 'report.json')) as fh:
                published = json.load(fh)
            self.assertEqual(len(published['paired']), 1)
            self.assertAlmostEqual(published['paired'][0]['median_ratio_go_over_rust'], 2.0)

    def test_a_refused_run_exits_nonzero_and_writes_nothing(self):
        with tempfile.TemporaryDirectory() as tmp:
            bad = doc('go')
            bad['checks'][0]['passed'] = False
            paths = {}
            for name, payload in (('identity', identity()), ('go', bad), ('rust', doc('rust'))):
                paths[name] = os.path.join(tmp, f'{name}.json')
                with open(paths[name], 'w') as fh:
                    json.dump(payload, fh)
            out = os.path.join(tmp, 'out')
            argv = sys.argv
            sys.argv = ['report.py', '--identity', paths['identity'], '--go', paths['go'],
                        '--rust', paths['rust'], '--out', out, '--no-plots']
            try:
                with self.assertRaises(SystemExit) as cm:
                    report.main()
                self.assertEqual(cm.exception.code, 1)
            finally:
                sys.argv = argv
            self.assertFalse(os.path.exists(os.path.join(out, 'REPORT.md')))


class TestFormatting(unittest.TestCase):
    def test_times_are_rendered_in_a_readable_unit(self):
        self.assertEqual(report.fmt_ns(1.5), '1.5 ns')
        self.assertEqual(report.fmt_ns(1500), '1.500 us')
        self.assertEqual(report.fmt_ns(1.5e6), '1.500 ms')
        self.assertEqual(report.fmt_ns(1.5e9), '1.500 s')

    def test_a_tiny_p_value_is_not_rendered_as_zero(self):
        self.assertEqual(report.fmt_p(1e-9), '<0.0001')

    def test_a_missing_rate_is_a_dash(self):
        self.assertEqual(report.fmt_rate(None), '—')
        self.assertEqual(report.fmt_rate(float('nan')), '—')
        self.assertTrue(report.fmt_rate(2 << 20).startswith('2.0'))

    def test_chart_spec_carries_dispersion_and_sample_counts(self):
        g, r = doc('go'), doc('rust')
        gc_, rc = report.collect(g), report.collect(r)
        paired, _ = report.pair(gc_, rc, report.summarize(gc_), report.summarize(rc))
        spec = report.chart_spec('key', paired, identity(), g['config'])
        self.assertEqual(spec['series'], ['Go core', 'Rust core'])
        self.assertEqual(len(spec['cells']), 1)
        for cell in spec['cells'][0]:
            self.assertIn('n', cell)
            self.assertTrue(math.isfinite(cell['value']))
            self.assertGreaterEqual(cell['high'], cell['value'])
        self.assertTrue(any('No overall ranking' in f or 'no overall ranking' in f
                            for f in spec['footer']))


if __name__ == '__main__':
    unittest.main()
