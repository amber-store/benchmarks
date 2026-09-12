"""Render recorded chart specifications with Matplotlib, without recomputing statistics."""
import io
import json
import math
import sys
import textwrap

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Patch

PALETTE = ['#4477aa', '#ee6677', '#228833', '#ccbb44', '#66ccee', '#aa3377', '#bbbbbb']


def validate_report(report):
    """Reject failed or inconsistent reports before comparing performance."""
    if not report.get('validity', {}).get('valid'):
        raise ValueError('The report is invalid; performance plots are disabled.')
    if report.get('errors') or report.get('skipped_backends') or report.get('tools', {}).get('missing'):
        raise ValueError('The report contains errors or missing backends.')
    if not isinstance(report.get('cross_checks'), list):
        raise ValueError('Missing cross-core checks.')
    for check in report['cross_checks']:
        if check.get('passed') is not True:
            raise ValueError('A cross-core check failed.')
    runs = report.get('runs', [])
    if not runs:
        raise ValueError('No raw runs.')
    for run in runs:
        if run.get('error') or not run.get('verifications') or not run.get('ops'):
            raise ValueError('Incomplete raw run.')
        if any(v.get('passed') is not True for v in run['verifications']):
            raise ValueError('A verification failed.')
        if any(op.get('status') not in ('ok', 'unsupported')
               or (op.get('status') == 'unsupported' and op.get('wall_ns') is not None)
               for op in run['ops']):
            raise ValueError('A raw operation failed.')
    if any(row.get('failed', 0) or row.get('excluded_invalid_samples', 0)
           for row in report.get('summary', [])):
        raise ValueError('Summary includes invalid samples.')


def figure(spec):
    categories, series, cells = spec['categories'], spec['series'], spec['cells']
    if not categories or not series or len(cells) != len(categories):
        raise ValueError('chart dimensions do not match')
    values = []
    for row in cells:
        if len(row) != len(series):
            raise ValueError('chart series dimensions do not match')
        for cell in row:
            for value in (cell.get('value'), cell.get('high')):
                if value is not None:
                    if not math.isfinite(value) or value < 0:
                        raise ValueError('chart values must be finite and nonnegative')
                    values.append(value)
    if not values:
        raise ValueError('no measured values in chart')
    footer = [line for text in spec['footer'] for line in textwrap.wrap(text, 145)]
    subtitle = [line for text in spec['subtitle'] for line in textwrap.wrap(text, 135)]
    top = 1.0 + .18 * len(subtitle) + .22 * math.ceil(len(series) / 3)
    bottom = .65 + .15 * len(footer)
    body = max(1.0, len(categories) * (len(series) * .23 + .20))
    height = top + body + bottom
    with plt.rc_context({'svg.fonttype': 'none', 'svg.hashsalt': 'amber-benchmarks', 'font.size': 9}):
        fig = plt.figure(figsize=(14, height))
        ax = fig.add_axes([.24, bottom / height, .71, body / height])
        ticks, y = [], 0.0
        limit = max(values) or 1.0
        for category, row in zip(categories, cells):
            ticks.append(y + (len(series) - 1) / 2)
            for index, cell in enumerate(row):
                value = cell.get('value')
                label = cell['label']
                if value is None:
                    ax.text(0, y, f'{series[index]} — {label}', va='center', color='#555555', fontsize=8)
                else:
                    ax.barh(y, value, height=.72, color=PALETTE[index % len(PALETTE)])
                    high = cell.get('high')
                    if high is not None and high != value:
                        ax.plot([high, high], [y - .36, y + .36], color='#222222', linewidth=1)
                    count = '' if cell.get('n') is None else f"; n={cell['n']}"
                    ax.text(max(value, high or 0) + limit * .012, y, label + count,
                            va='center', fontsize=8)
                y += 1
            y += .85
        ax.set_yticks(ticks, categories, fontsize=9)
        ax.set_ylim(y - .3, -1)
        ax.set_xlim(0, limit * 1.30)
        ax.grid(axis='x', alpha=.2)
        ax.set_axisbelow(True)
        ax.spines[['top', 'right', 'left']].set_visible(False)
        ax.tick_params(axis='y', length=0)
        ax.legend(handles=[Patch(color=PALETTE[i % len(PALETTE)], label=s)
                           for i, s in enumerate(series)], ncols=min(3, len(series)),
                  loc='lower left', bbox_to_anchor=(0, 1.01), frameon=False)
        fig.text(.02, 1 - .22 / height, spec['title'], fontsize=14, va='top')
        for i, line in enumerate(subtitle):
            fig.text(.02, 1 - (.54 + i * .18) / height, line, fontsize=9, va='top')
        for i, line in enumerate(footer):
            fig.text(.02, (bottom - .43 - i * .15) / height, line, fontsize=8, va='top')
    return fig


def render_svg(spec):
    fig = figure(spec)
    output = io.StringIO()
    with plt.rc_context({'svg.fonttype': 'none', 'svg.hashsalt': 'amber-benchmarks'}):
        fig.savefig(output, format='svg', metadata={'Date': None})
    plt.close(fig)
    return output.getvalue()


if __name__ == '__main__':
    print(render_svg(json.loads(sys.argv[1])))
