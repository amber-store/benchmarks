"""Tests for the core-operation report: the statistics and the validity gate.

The gate is the part worth testing hardest. Every rule in it exists because
publishing a comparison that has not earned it is worse than publishing
nothing, so each rule gets a test that proves it actually refuses — and the
refusals that matter most are the ones an independent probe got past an
earlier version of this code: empty check lists, two documents whose samples
shared a label while describing different work, and a shared case that
existed on one side only and quietly became "an operation one core does not
export".
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
import validate as validation  # noqa: E402

Invalid = validation.Invalid


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


class TestBootstrap(unittest.TestCase):
    def test_empty_sample_is_an_error(self):
        with self.assertRaises(ValueError):
            stats.bootstrap_ratio_ci([], [1.0])

    def test_interval_brackets_the_point_estimate(self):
        a = [10.0, 11.0, 12.0, 10.5, 11.5]
        b = [5.0, 5.5, 6.0, 5.25, 5.75]
        lo, hi = stats.bootstrap_ratio_ci(a, b)
        point = stats.median(a) / stats.median(b)
        self.assertLessEqual(lo, point)
        self.assertGreaterEqual(hi, point)

    def test_is_reproducible(self):
        a, b = [1.0, 2.0, 3.0], [1.0, 1.5, 2.0]
        self.assertEqual(stats.bootstrap_ratio_ci(a, b), stats.bootstrap_ratio_ci(a, b))

    def test_identical_samples_give_an_interval_around_one(self):
        xs = [1.0, 2.0, 3.0, 4.0, 5.0]
        lo, hi = stats.bootstrap_ratio_ci(xs, list(xs))
        self.assertLessEqual(lo, 1.0)
        self.assertGreaterEqual(hi, 1.0)


# ---------------------------------------------------------------------------
# Documents the gate is asked about
# ---------------------------------------------------------------------------

DIMS = {'item_bytes': 10, 'items': 10, 'content': 'random', 'workers': 1}


def sample(op='key.new', workload='w', rep=0, wall=1000, **over):
    s = {
        'group': op.split('.')[0], 'op': op, 'workload': workload, 'threads': 1,
        'dims': dict(DIMS), 'rep': rep, 'ops': 10, 'bytes': 100,
        'bytes_kind': 'payload', 'wall_ns': wall, 'cpu_user_ns': wall, 'cpu_sys_ns': 0,
        'max_rss_kib': 1000, 'checksum': 4242, 'stable': True, 'cross_checksum': True,
        'status': 'ok',
    }
    s.update(over)
    return s


def doc(core, **over):
    base = {
        'schema': validation.SAMPLE_SCHEMA,
        'core': core,
        'profile': 'standard',
        'config': {f: v for f, v in [
            ('name', 'standard'), ('seed', 1), ('reps', 2), ('warmup', 0),
            ('threads_single', 1), ('threads_multi', 8), ('corpus_bytes', 1),
            ('tree_files', 1), ('tree_wide', 1), ('tree_depth', 1),
            ('synthetic_wide', 1), ('store_objects', 1), ('segment_bytes', 1),
            ('ref_records', 1), ('inbox_packs', 1), ('batch_ops', 1),
            ('payload_total', 1), ('tree_widths', [1]), ('tree_depths', [1]),
            ('fan_outs', [1])]},
        'identity': {
            'toolchain': 'test',
            'core_revision': ('a' if core == 'go' else 'b') * 40,
            'core_dirty': False,
            'driver_sha256': ('e' if core == 'go' else 'f') * 64,
        },
        'environment': {'host': 'h', 'os': 'linux', 'arch': 'amd64', 'num_cpu': 8,
                        'auto_parallelism': 8, 'forced_gc_before_rep': core == 'go',
                        'scratch_fs': 'ext4', 'xattrs': False},
        'started_at': '', 'finished_at': '',
        'checks': [{'id': 'c/1', 'group': 'key', 'op': 'key.new', 'passed': True,
                    'detail': '', 'digest': 'abc', 'comparable': True}],
        'samples': [sample(rep=0, wall=1000 if core == 'go' else 500),
                    sample(rep=1, wall=1100 if core == 'go' else 550)],
        'counters': [],
        'unsupported': [],
        'encodings': [],
        'wire_inputs': [],
        'blackhole': 1,
    }
    base.update(over)
    return base


def merged(core, **over):
    return validation.merge_passes([doc(core, **over)], core)


def identity(**over):
    base = {
        'schema': 'amber-core-ops/identity/2',
        'pin_verified': True,
        'flake_lock': {'amber_go_src': 'a' * 40, 'amber_rust_src': 'b' * 40},
        'harness': {'revision': 'c' * 40, 'dirty': False, 'sources_sha256': 'd'},
        'cores': {
            'go': {'source': 'pinned', 'revision': 'a' * 40, 'dirty': False,
                   'module': 'm', 'module_tree_vs_pinned_source': 'identical',
                   'driver': 'x', 'driver_sha256': 'e' * 64},
            'rust': {'source': 'pinned', 'revision': 'b' * 40, 'dirty': False,
                     'module': 'm', 'cargo_lock_requested_rev': 'b' * 40,
                     'cargo_lock_resolved_rev': 'b' * 40,
                     'cargo_resolved_source': 'git+x?rev=b#b',
                     'driver': 'y', 'driver_sha256': 'f' * 64},
        },
        'environment': {'host': 'h', 'kernel': 'k', 'cpu_model': 'c',
                        'cpu_count': 8, 'cpu_set': '0-7', 'mem_total_kb': 1024,
                        'scratch': '/s', 'scratch_fs': 'ext4'},
    }
    base.update(over)
    return base


def check(identity_doc=None, go=None, rust=None, manifest=None):
    return validation.validate(identity_doc or identity(), go or merged('go'),
                               rust or merged('rust'), manifest)


class TestValidity(unittest.TestCase):
    def test_a_clean_pair_validates(self):
        ok = check()
        self.assertTrue(any('correctness check passed' in s for s in ok))
        self.assertTrue(any('digests are identical' in s for s in ok))

    def test_a_wrong_schema_is_refused(self):
        with self.assertRaisesRegex(Invalid, 'schema'):
            validation.merge_passes([doc('go', schema='other')], 'go')

    def test_a_configuration_difference_is_refused(self):
        r = doc('rust')
        r['config']['seed'] = 99
        with self.assertRaisesRegex(Invalid, 'disagree on config.seed'):
            check(rust=validation.merge_passes([r], 'rust'))

    def test_every_config_field_is_compared_not_a_hand_written_list(self):
        # A field nobody thought to enumerate still has to match.
        g, r = doc('go'), doc('rust')
        g['config']['a_new_knob'] = 1
        r['config']['a_new_knob'] = 2
        with self.assertRaisesRegex(Invalid, 'a_new_knob'):
            check(go=validation.merge_passes([g], 'go'),
                  rust=validation.merge_passes([r], 'rust'))

    def test_a_missing_revision_is_refused(self):
        ident = identity()
        ident['cores']['rust']['revision'] = 'short'
        with self.assertRaisesRegex(Invalid, 'revision'):
            check(ident)

    def test_a_missing_driver_hash_is_refused(self):
        ident = identity()
        ident['cores']['go']['driver_sha256'] = ''
        with self.assertRaisesRegex(Invalid, 'executable hash'):
            check(ident)

    def test_an_identity_that_does_not_describe_the_samples_is_refused(self):
        # An identity file could otherwise claim any revision at all and the
        # samples beside it would never contradict it.
        ident = identity()
        ident['cores']['go']['revision'] = '9' * 40
        with self.assertRaisesRegex(Invalid, 'core_revision'):
            check(ident)

    def test_an_identity_naming_another_executable_is_refused(self):
        ident = identity()
        ident['cores']['rust']['driver_sha256'] = '0' * 64
        with self.assertRaisesRegex(Invalid, 'driver_sha256'):
            check(ident)

    def test_an_empty_check_list_is_refused(self):
        # The probe that got a comparison out of a run with no checks at all.
        with self.assertRaisesRegex(Invalid, 'no correctness checks'):
            check(go=merged('go', checks=[]))

    def test_an_empty_sample_list_is_refused(self):
        with self.assertRaisesRegex(Invalid, 'no samples'):
            check(go=merged('go', samples=[]))

    def test_a_repeated_check_id_is_refused(self):
        g = doc('go')
        g['checks'] = g['checks'] + [dict(g['checks'][0])]
        with self.assertRaisesRegex(Invalid, 'more than once'):
            check(go=validation.merge_passes([g], 'go'))

    def test_a_comparable_check_with_no_digest_is_refused(self):
        g, r = doc('go'), doc('rust')
        g['checks'][0]['digest'] = ''
        r['checks'][0]['digest'] = ''
        with self.assertRaisesRegex(Invalid, 'no digest'):
            check(go=validation.merge_passes([g], 'go'),
                  rust=validation.merge_passes([r], 'rust'))

    def test_a_failed_check_is_refused(self):
        g = doc('go')
        g['checks'][0]['passed'] = False
        with self.assertRaisesRegex(Invalid, 'failing checks'):
            check(go=validation.merge_passes([g], 'go'))

    def test_a_cross_core_digest_mismatch_is_refused(self):
        r = doc('rust')
        r['checks'][0]['digest'] = 'different'
        with self.assertRaisesRegex(Invalid, 'digests differ'):
            check(rust=validation.merge_passes([r], 'rust'))

    def test_a_comparable_check_on_one_side_only_is_refused(self):
        r = doc('rust')
        r['checks'] = [{'id': 'other', 'group': 'key', 'op': 'key.new', 'passed': True,
                        'detail': '', 'digest': 'abc', 'comparable': True}]
        with self.assertRaisesRegex(Invalid, 'one core only'):
            check(rust=validation.merge_passes([r], 'rust'))

    def test_measuring_something_declared_unsupported_is_refused(self):
        g = doc('go')
        g['unsupported'] = [{'op': 'key.new', 'workload': '', 'reason': 'r'}]
        with self.assertRaisesRegex(Invalid, 'declares and measures'):
            check(go=validation.merge_passes([g], 'go'))

    def test_an_unsupported_declaration_without_a_reason_is_refused(self):
        g = doc('go')
        g['unsupported'] = [{'op': 'other.op', 'workload': '', 'reason': ''}]
        with self.assertRaisesRegex(Invalid, 'without a reason'):
            check(go=validation.merge_passes([g], 'go'))

    def test_a_zero_time_sample_is_refused(self):
        g = doc('go')
        g['samples'][0]['wall_ns'] = 0
        with self.assertRaisesRegex(Invalid, 'non-positive'):
            check(go=validation.merge_passes([g], 'go'))

    def test_a_non_finite_sample_is_refused(self):
        g = doc('go')
        g['samples'][0]['wall_ns'] = float('nan')
        with self.assertRaisesRegex(Invalid, 'non-finite'):
            check(go=validation.merge_passes([g], 'go'))

    def test_bytes_without_a_kind_are_refused(self):
        g, r = doc('go'), doc('rust')
        for d in (g, r):
            for s in d['samples']:
                s['bytes_kind'] = ''
        with self.assertRaisesRegex(Invalid, 'what they count'):
            check(go=validation.merge_passes([g], 'go'),
                  rust=validation.merge_passes([r], 'rust'))

    def test_a_failed_sample_status_is_refused(self):
        g = doc('go')
        g['samples'][0]['status'] = 'error'
        with self.assertRaisesRegex(Invalid, 'status'):
            check(go=validation.merge_passes([g], 'go'))

    def test_a_missing_repetition_is_refused(self):
        g = doc('go')
        g['samples'] = g['samples'][:1]
        with self.assertRaisesRegex(Invalid, 'repetitions'):
            check(go=validation.merge_passes([g], 'go'))

    def test_a_duplicated_repetition_index_is_refused(self):
        g = doc('go')
        g['samples'] = [sample(rep=0), sample(rep=0)]
        with self.assertRaisesRegex(Invalid, 'passes overlap'):
            check(go=validation.merge_passes([g], 'go'))

    # -- the probes that got past an earlier version ----------------------

    def test_a_shared_label_over_different_work_is_refused(self):
        """Go counting one operation over 64 bytes, Rust a hundred over 4096.

        Both documents parsed, both had the same label, and the earlier gate
        published the comparison.
        """
        g, r = doc('go'), doc('rust')
        for s in g['samples']:
            s['ops'], s['bytes'] = 1, 64
        for s in r['samples']:
            s['ops'], s['bytes'] = 100, 4096
        with self.assertRaisesRegex(Invalid, 'not the same experiment'):
            check(go=validation.merge_passes([g], 'go'),
                  rust=validation.merge_passes([r], 'rust'))

    def test_a_dimension_difference_under_a_shared_label_is_refused(self):
        r = doc('rust')
        for s in r['samples']:
            s['dims'] = dict(DIMS, item_bytes=999)
        with self.assertRaisesRegex(Invalid, 'not the same experiment'):
            check(rust=validation.merge_passes([r], 'rust'))

    def test_a_case_changing_shape_between_repetitions_is_refused(self):
        g = doc('go')
        g['samples'][1]['ops'] = 999
        with self.assertRaisesRegex(Invalid, 'between repetitions'):
            check(go=validation.merge_passes([g], 'go'))

    def test_a_missing_shared_case_is_a_broken_pair_not_an_unpaired_operation(self):
        """The workload vanished on one side; the operation did not."""
        g, r = doc('go'), doc('rust')
        g['samples'] += [sample(workload='w2', rep=0), sample(workload='w2', rep=1)]
        with self.assertRaisesRegex(Invalid, 'broken pair'):
            check(go=validation.merge_passes([g], 'go'),
                  rust=validation.merge_passes([r], 'rust'))

    def test_an_undeclared_single_core_operation_is_refused(self):
        g = doc('go')
        g['samples'] += [sample(op='gc.begin_write', rep=0, cross_checksum=False),
                         sample(op='gc.begin_write', rep=1, cross_checksum=False)]
        with self.assertRaisesRegex(Invalid, 'must be declared, not inferred'):
            check(go=validation.merge_passes([g], 'go'))

    def test_a_declared_single_core_operation_is_allowed(self):
        g, r = doc('go'), doc('rust')
        g['samples'] += [sample(op='gc.begin_write', rep=0, cross_checksum=False),
                         sample(op='gc.begin_write', rep=1, cross_checksum=False)]
        r['unsupported'] = [{'op': 'gc.begin_write', 'workload': '',
                             'reason': 'the Rust collector keeps the gate internal'}]
        ok = check(go=validation.merge_passes([g], 'go'),
                   rust=validation.merge_passes([r], 'rust'))
        self.assertTrue(any('explicitly declares unsupported' in s for s in ok))

    # -- output agreement and identity of inputs --------------------------

    def test_an_unstable_stable_case_is_refused(self):
        g = doc('go')
        g['samples'][1]['checksum'] = 99
        with self.assertRaisesRegex(Invalid, 'declared stable'):
            check(go=validation.merge_passes([g], 'go'))

    def test_a_cross_core_output_difference_is_refused(self):
        r = doc('rust')
        for s in r['samples']:
            s['checksum'] = 7
        with self.assertRaisesRegex(Invalid, 'different output'):
            check(rust=validation.merge_passes([r], 'rust'))

    def test_a_run_requiring_no_output_agreement_at_all_is_refused(self):
        g, r = doc('go'), doc('rust')
        for d in (g, r):
            for s in d['samples']:
                s['cross_checksum'] = False
        with self.assertRaisesRegex(Invalid, 'no case requires'):
            check(go=validation.merge_passes([g], 'go'),
                  rust=validation.merge_passes([r], 'rust'))

    def test_a_single_core_case_cannot_require_cross_core_agreement(self):
        g, r = doc('go'), doc('rust')
        g['samples'] += [sample(op='gc.begin_write', rep=0), sample(op='gc.begin_write', rep=1)]
        r['unsupported'] = [{'op': 'gc.begin_write', 'workload': '', 'reason': 'Go-only'}]
        with self.assertRaisesRegex(Invalid, 'single-core case claims'):
            check(go=validation.merge_passes([g], 'go'),
                  rust=validation.merge_passes([r], 'rust'))

    def test_decoding_different_bytes_is_refused(self):
        g, r = doc('go'), doc('rust')
        g['wire_inputs'] = [{'producer': 'go', 'sha256': 'aa', 'bytes': 1, 'objects': 1}]
        r['wire_inputs'] = [{'producer': 'go', 'sha256': 'bb', 'bytes': 1, 'objects': 1}]
        with self.assertRaisesRegex(Invalid, 'different bytes'):
            check(go=validation.merge_passes([g], 'go'),
                  rust=validation.merge_passes([r], 'rust'))

    def test_a_producer_case_with_no_recorded_wire_input_is_refused(self):
        g, r = doc('go'), doc('rust')
        for d in (g, r):
            d['samples'] = [sample(op='amberpack.reader_all',
                                   workload='mixed/producer-go', rep=i) for i in (0, 1)]
        with self.assertRaisesRegex(Invalid, 'no wire inputs'):
            check(go=validation.merge_passes([g], 'go'),
                  rust=validation.merge_passes([r], 'rust'))

    def test_encoders_fed_different_input_are_refused(self):
        g, r = doc('go'), doc('rust')
        g['encodings'] = [{'op': 'e', 'workload': 'w', 'logical_bytes': 10,
                           'encoded_bytes': 5, 'items': 1}]
        r['encodings'] = [{'op': 'e', 'workload': 'w', 'logical_bytes': 99,
                           'encoded_bytes': 6, 'items': 1}]
        with self.assertRaisesRegex(Invalid, 'encoded different input'):
            check(go=validation.merge_passes([g], 'go'),
                  rust=validation.merge_passes([r], 'rust'))

    def test_different_encoded_sizes_are_allowed(self):
        g, r = doc('go'), doc('rust')
        g['encodings'] = [{'op': 'e', 'workload': 'w', 'logical_bytes': 10,
                           'encoded_bytes': 5, 'items': 1}]
        r['encodings'] = [{'op': 'e', 'workload': 'w', 'logical_bytes': 10,
                           'encoded_bytes': 7, 'items': 1}]
        ok = check(go=validation.merge_passes([g], 'go'),
                   rust=validation.merge_passes([r], 'rust'))
        self.assertTrue(any('never divided' in s for s in ok))

    # -- environment ------------------------------------------------------

    def test_different_automatic_parallelism_is_refused(self):
        r = doc('rust')
        r['environment']['auto_parallelism'] = 2
        with self.assertRaisesRegex(Invalid, 'auto_parallelism'):
            check(rust=validation.merge_passes([r], 'rust'))

    def test_different_xattr_support_is_refused(self):
        r = doc('rust')
        r['environment']['xattrs'] = True
        with self.assertRaisesRegex(Invalid, 'xattrs'):
            check(rust=validation.merge_passes([r], 'rust'))

    def test_a_missing_environment_field_is_refused(self):
        r = doc('rust')
        del r['environment']['auto_parallelism']
        with self.assertRaisesRegex(Invalid, 'does not record'):
            check(rust=validation.merge_passes([r], 'rust'))

    # -- merging passes ---------------------------------------------------

    def test_overlapping_passes_are_refused(self):
        one = doc('go', samples=[sample(rep=0)])
        with self.assertRaisesRegex(Invalid, 'passes overlap'):
            validation.merge_passes([one, doc('go', samples=[sample(rep=0)])], 'go')

    def test_passes_that_disagree_on_the_configuration_are_refused(self):
        two = doc('go')
        two['config']['seed'] = 5
        with self.assertRaisesRegex(Invalid, 'disagrees with pass 0'):
            validation.merge_passes([doc('go'), two], 'go')

    def test_two_passes_make_one_document(self):
        a = doc('go', samples=[sample(rep=0, wall=1000)])
        b = doc('go', samples=[sample(rep=1, wall=1100)])
        m = validation.merge_passes([a, b], 'go')
        self.assertEqual(len(m['samples']), 2)
        self.assertEqual(m['passes'], 2)
        ok = check(go=m)
        self.assertTrue(any('opposite core order' in s for s in ok))

    # -- the coverage manifest --------------------------------------------

    def test_a_manifest_workload_that_was_not_measured_is_refused(self):
        manifest = {'operations': {'key.new': ['w', 'w2']},
                    'single_core': {'go': [], 'rust': []}, 'checks': []}
        with self.assertRaisesRegex(Invalid, 'does not expect|missing'):
            check(manifest=manifest)

    def test_a_measured_operation_the_manifest_omits_is_refused(self):
        manifest = {'operations': {}, 'single_core': {'go': [], 'rust': []}, 'checks': []}
        with self.assertRaisesRegex(Invalid, 'defines no operations'):
            check(manifest=manifest)

    def test_an_unlisted_operation_is_refused(self):
        manifest = {'operations': {'other.op': ['w']},
                    'single_core': {'go': [], 'rust': []}, 'checks': []}
        with self.assertRaisesRegex(Invalid, 'expects paired samples'):
            check(manifest=manifest)

    def test_a_manifest_check_no_run_recorded_is_refused(self):
        manifest = {'operations': {'key.new': ['w']},
                    'single_core': {'go': [], 'rust': []}, 'checks': ['never/recorded']}
        with self.assertRaisesRegex(Invalid, 'no run recorded'):
            check(manifest=manifest)

    def test_a_matching_manifest_validates(self):
        manifest = {'operations': {'key.new': ['w']},
                    'single_core': {'go': [], 'rust': []}, 'checks': ['c/1']}
        ok = check(manifest=manifest)
        self.assertTrue(any('exactly the manifest' in s for s in ok))

    def test_an_unverified_pin_is_reported_rather_than_hidden(self):
        ok = check(identity(pin_verified=False))
        self.assertTrue(any('WARNING' in s for s in ok))


class TestPairing(unittest.TestCase):
    def test_pairs_are_built_per_operation_and_workload(self):
        g, r = merged('go'), merged('rust')
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
        gc_ = report.collect(validation.merge_passes([g], 'go'))
        rc = report.collect(merged('rust'))
        paired, unpaired = report.pair(gc_, rc, report.summarize(gc_), report.summarize(rc))
        self.assertEqual(len(paired), 1)
        self.assertEqual([u['op'] for u in unpaired], ['gc.begin_write'])
        self.assertEqual(unpaired[0]['core'], 'go')

    def test_a_rate_is_only_derived_where_its_denominator_exists(self):
        g = doc('go')
        for s in g['samples']:
            s['bytes'] = 0
            s['dims'] = {'items': 1}
        summary = report.summarize(report.collect(validation.merge_passes([g], 'go')))
        row = list(summary.values())[0]
        self.assertNotIn('bytes_per_s', row)
        self.assertNotIn('entries_per_s', row)
        self.assertIn('ops_per_s', row)

    def test_entry_and_object_rates_use_the_recorded_dimensions(self):
        g = doc('go')
        for s in g['samples']:
            s['wall_ns'] = 1_000_000_000
            s['dims'] = dict(DIMS, entries=7, objects=3)
        summary = report.summarize(report.collect(validation.merge_passes([g], 'go')))
        row = list(summary.values())[0]
        self.assertAlmostEqual(row['entries_per_s']['median'], 7.0)
        self.assertAlmostEqual(row['objects_per_s']['median'], 3.0)

    def test_variation_is_dispersion_relative_to_the_median(self):
        g = doc('go')
        g['samples'] = [sample(rep=0, wall=1000), sample(rep=1, wall=1000)]
        summary = report.summarize(report.collect(validation.merge_passes([g], 'go')))
        self.assertEqual(list(summary.values())[0]['variation'], 0.0)


class TestScaling(unittest.TestCase):
    def rows(self, values, series='plain', field='entries', op='fstree.dir_builder'):
        paired = []
        for v in values:
            g = doc('go', samples=[sample(op=op, workload=f'w{v}', rep=i,
                                          dims={field: v, 'sweep': field, 'series': series})
                                   for i in (0, 1)])
            r = doc('rust', samples=[sample(op=op, workload=f'w{v}', rep=i,
                                            dims={field: v, 'sweep': field, 'series': series})
                                     for i in (0, 1)])
            gc_ = report.collect(validation.merge_passes([g], 'go'))
            rc = report.collect(validation.merge_passes([r], 'rust'))
            paired += report.pair(gc_, rc, report.summarize(gc_), report.summarize(rc))[0]
        return paired

    def test_a_declared_sweep_becomes_a_curve(self):
        sweeps = report.sweeps(self.rows([1, 2, 4]))
        self.assertEqual(len(sweeps), 1)
        self.assertEqual(sweeps[0]['dimension'], 'entries')
        self.assertEqual([v for v, _ in sweeps[0]['points']], [1, 2, 4])

    def test_two_points_are_not_a_curve(self):
        self.assertEqual(report.sweeps(self.rows([1, 2])), [])

    def test_different_series_are_not_mixed(self):
        rows = self.rows([1, 2, 4], series='hit') + self.rows([1, 2, 4], series='miss')
        sweeps = report.sweeps(rows)
        self.assertEqual(len(sweeps), 2)
        self.assertEqual({s['series'] for s in sweeps}, {'hit', 'miss'})

    def test_an_undeclared_sweep_is_not_guessed(self):
        paired = self.rows([1, 2, 4])
        for r in paired:
            r['dims'] = {'entries': r['dims']['entries']}
        self.assertEqual(report.sweeps(paired), [])

    def test_per_unit_divides_a_per_item_size_by_the_whole_call(self):
        row = {'dims': {'item_bytes': 64, 'items': 10, 'sweep': 'item_bytes'}}
        self.assertEqual(report.sweep_total('item_bytes', row), 640)
        self.assertEqual(report.sweep_total('entries', {'dims': {'entries': 5}}), 5)


class TestOutput(unittest.TestCase):
    def run_report(self, tmp, go_doc=None, rust_doc=None, expect_exit=None):
        paths = {}
        for name, payload in (('identity', identity()), ('go', go_doc or doc('go')),
                              ('rust', rust_doc or doc('rust'))):
            paths[name] = os.path.join(tmp, f'{name}.json')
            with open(paths[name], 'w') as fh:
                json.dump(payload, fh)
        out = os.path.join(tmp, 'out')
        argv = sys.argv
        sys.argv = ['report.py', '--identity', paths['identity'], '--go', paths['go'],
                    '--rust', paths['rust'], '--out', out, '--no-plots', '--no-manifest']
        try:
            if expect_exit is None:
                report.main()
            else:
                with self.assertRaises(SystemExit) as cm:
                    report.main()
                self.assertEqual(cm.exception.code, expect_exit)
        finally:
            sys.argv = argv
        return out

    def test_a_full_run_writes_every_artefact(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = self.run_report(tmp)
            for name in ('REPORT.md', 'report.json', 'samples.csv', 'summary.csv',
                         'paired.csv', 'scaling.csv', 'checks.csv', 'counters.csv',
                         'encodings.csv', 'unsupported.csv'):
                self.assertTrue(os.path.exists(os.path.join(out, name)), name)
            with open(os.path.join(out, 'REPORT.md')) as fh:
                text = fh.read()
            self.assertIn('no overall ranking', text.lower())
            self.assertIn('a' * 40, text)  # the Go core revision, in full
            self.assertIn('b' * 40, text)  # and the Rust one
            self.assertIn('c' * 40, text)  # and the harness revision, separately
            # The resident-set section must not claim to be a per-case peak.
            self.assertIn('not a', text.split('## Resident set')[1][:800])
            with open(os.path.join(out, 'report.json')) as fh:
                published = json.load(fh)
            self.assertEqual(len(published['paired']), 1)
            self.assertAlmostEqual(published['paired'][0]['median_ratio_go_over_rust'], 2.0)

    def test_a_refused_run_exits_nonzero_and_writes_nothing(self):
        with tempfile.TemporaryDirectory() as tmp:
            bad = doc('go')
            bad['checks'][0]['passed'] = False
            out = self.run_report(tmp, go_doc=bad, expect_exit=1)
            self.assertFalse(os.path.exists(os.path.join(out, 'REPORT.md')))

    def test_the_samples_csv_carries_every_dimension(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = self.run_report(tmp)
            with open(os.path.join(out, 'samples.csv')) as fh:
                header = fh.readline().strip().split(',')
            for field in ('item_bytes', 'content', 'entries', 'depth', 'width',
                          'sweep', 'series', 'checksum', 'bytes_kind'):
                self.assertIn(field, header)


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

    def test_count_rates_carry_their_magnitude(self):
        self.assertEqual(report.fmt_count_rate(1500), '1.50 k/s')
        self.assertEqual(report.fmt_count_rate(2.5e9), '2.50 G/s')

    def test_chart_spec_carries_dispersion_and_sample_counts(self):
        g, r = merged('go'), merged('rust')
        gc_, rc = report.collect(g), report.collect(r)
        paired, _ = report.pair(gc_, rc, report.summarize(gc_), report.summarize(rc))
        spec = report.group_chart('key', paired, identity(), g['config'], 1, 1)
        self.assertEqual(spec['series'], ['Go core', 'Rust core'])
        self.assertEqual(len(spec['cells']), 1)
        for cell in spec['cells'][0]:
            self.assertIn('n', cell)
            self.assertTrue(math.isfinite(cell['value']))
            self.assertGreaterEqual(cell['high'], cell['value'])
        self.assertTrue(any('no overall ranking' in f.lower() for f in spec['footer']))

    def test_pages_hold_operations_of_a_comparable_scale(self):
        def row(ns):
            return {'op': 'a.b', 'workload': 'w', 'threads': 1,
                    'go': {'time': {'median': ns, 'p75': ns, 'n': 1}},
                    'rust': {'time': {'median': ns, 'p75': ns, 'n': 1}}}
        pages = report.paginate_by_magnitude([row(1), row(10), row(1e9)])
        self.assertEqual(len(pages), 2)
        self.assertEqual(len(pages[0]), 2)


if __name__ == '__main__':
    unittest.main()
