import copy
import unittest
import xml.etree.ElementTree as ET
from amber_bench_plot import figure, render_svg, validate_report
import matplotlib.pyplot as plt


class PlotTests(unittest.TestCase):
    def spec(self):
        return {'title': 'blob/backup — bytes', 'subtitle': ['median bytes'],
                'footer': ['Warm cache; small sample'], 'categories': ['push_noop'],
                'series': ['amber-go', 'unsupported-backend'],
                'cells': [[{'value': 0, 'high': 0, 'label': '0 B', 'n': 3},
                           {'value': None, 'high': None, 'label': 'unsupported', 'n': None}]]}

    def test_zero_is_a_measurement_and_unsupported_is_not(self):
        fig = figure(self.spec())
        self.assertEqual([p.get_width() for p in fig.axes[0].patches], [0])
        plt.close(fig)
        svg = render_svg(self.spec())
        text = ''.join(ET.fromstring(svg).itertext())
        self.assertIn('0 B; n=3', text)
        self.assertIn('unsupported-backend — unsupported', text)
        self.assertIn('Warm cache', text)
        self.assertEqual(svg, render_svg(self.spec()))

    def test_negative_or_nonfinite_values_are_rejected(self):
        for value in [-1, float('nan'), float('inf')]:
            spec = self.spec()
            spec['cells'][0][0]['value'] = value
            with self.assertRaises(ValueError):
                figure(spec)

    def test_valid_flag_does_not_override_failed_raw_checks(self):
        report = {'validity': {'valid': True}, 'cross_checks': [{'passed': True}],
                  'runs': [{'ops': [{'status': 'ok'}], 'verifications': [{'passed': True}]}],
                  'summary': []}
        validate_report(report)
        broken = copy.deepcopy(report)
        broken['cross_checks'][0]['passed'] = False
        with self.assertRaises(ValueError):
            validate_report(broken)
        broken = copy.deepcopy(report)
        broken['runs'][0]['verifications'][0]['passed'] = False
        with self.assertRaises(ValueError):
            validate_report(broken)
