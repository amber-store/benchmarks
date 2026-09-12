"""Statistics for the core-operation benchmark, recomputed from raw samples.

Nothing here reads a summary another program wrote: every number in the
report is derived from the per-repetition samples the two drivers emitted.
The tests in test_report.py pin the behaviour of each function.
"""
import math


def median(xs):
    s = sorted(xs)
    n = len(s)
    if n == 0:
        raise ValueError('median of an empty sample')
    mid = n // 2
    return s[mid] if n % 2 else (s[mid - 1] + s[mid]) / 2.0


def quantile(xs, q):
    """The linear-interpolation quantile (the 'type 7' definition)."""
    if not 0.0 <= q <= 1.0:
        raise ValueError('quantile out of range')
    s = sorted(xs)
    n = len(s)
    if n == 0:
        raise ValueError('quantile of an empty sample')
    if n == 1:
        return float(s[0])
    pos = q * (n - 1)
    lo = math.floor(pos)
    hi = math.ceil(pos)
    return float(s[lo] + (s[hi] - s[lo]) * (pos - lo))


def mad(xs):
    """Median absolute deviation: dispersion that a single outlier cannot move."""
    m = median(xs)
    return median([abs(x - m) for x in xs])


def describe(xs):
    """The dispersion summary every reported measurement carries."""
    s = sorted(xs)
    m = median(s)
    return {
        'n': len(s),
        'min': float(s[0]),
        'p25': quantile(s, 0.25),
        'median': float(m),
        'p75': quantile(s, 0.75),
        'max': float(s[-1]),
        'mad': float(mad(s)),
        'mean': sum(s) / len(s),
    }


class SplitMix64:
    """The generator the drivers use for their fixtures, reused here so the
    bootstrap resampling is reproducible without depending on Python's RNG."""

    MASK = (1 << 64) - 1

    def __init__(self, seed):
        self.s = seed & self.MASK

    def next(self):
        self.s = (self.s + 0x9E3779B97F4A7C15) & self.MASK
        z = self.s
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & self.MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & self.MASK
        return z ^ (z >> 31)

    def below(self, bound):
        return self.next() % bound


def bootstrap_ratio_ci(a, b, iterations=2000, seed=0x5EEDC0DE, level=0.95):
    """A percentile bootstrap interval for median(a)/median(b).

    Both samples are resampled with replacement. With the handful of
    repetitions a benchmark profile takes, this interval is wide on purpose:
    it says how little a few repetitions can settle, which is exactly what a
    reader needs to know before treating a ratio as a result.
    """
    if not a or not b:
        raise ValueError('bootstrap of an empty sample')
    rng = SplitMix64(seed)
    ratios = []
    for _ in range(iterations):
        ra = [a[rng.below(len(a))] for _ in range(len(a))]
        rb = [b[rng.below(len(b))] for _ in range(len(b))]
        mb = median(rb)
        if mb <= 0:
            continue
        ratios.append(median(ra) / mb)
    if not ratios:
        return (float('nan'), float('nan'))
    ratios.sort()
    lo = quantile(ratios, (1.0 - level) / 2.0)
    hi = quantile(ratios, 1.0 - (1.0 - level) / 2.0)
    return (lo, hi)


def rank_sums(a, b):
    """Midranks of the pooled sample, returned as (U_a, tie correction term)."""
    pooled = sorted([(v, 0) for v in a] + [(v, 1) for v in b])
    ranks = [0.0] * len(pooled)
    ties = []
    i = 0
    while i < len(pooled):
        j = i
        while j + 1 < len(pooled) and pooled[j + 1][0] == pooled[i][0]:
            j += 1
        avg = (i + j) / 2.0 + 1.0
        for k in range(i, j + 1):
            ranks[k] = avg
        ties.append(j - i + 1)
        i = j + 1
    ra = sum(r for r, (_, side) in zip(ranks, pooled) if side == 0)
    n = len(a)
    u_a = ra - n * (n + 1) / 2.0
    return u_a, ties


def _exact_u_counts(n, m):
    """Counts of rank arrangements per value of U, by dynamic programming.

    `counts[u]` is the number of ways to interleave n items of one sample and
    m of the other so that U equals u. The recurrence is the standard one,
    f(i, j, u) = f(i-1, j, u-j) + f(i, j-1, u): the next position in the
    merged order is taken either from the first sample (which is preceded by
    j items of the second, adding j to U) or from the second.
    """
    size = n * m + 1
    # row[j] is the distribution for (i, j); rebuilt as i advances.
    row = [[1] + [0] * (size - 1) for _ in range(m + 1)]
    for i in range(1, n + 1):
        new_row = [[1] + [0] * (size - 1)]  # j == 0
        for j in range(1, m + 1):
            prev_i = row[j]        # f(i-1, j, .)
            prev_j = new_row[j - 1]  # f(i, j-1, .)
            cur = [0] * size
            for u in range(size):
                v = prev_j[u]
                if u >= j:
                    v += prev_i[u - j]
                cur[u] = v
            new_row.append(cur)
        row = new_row
    return row[m]


def mann_whitney_u(a, b):
    """Two-sided Mann-Whitney U test.

    Returns (U for `a`, p-value, method). With no ties and a small sample —
    which is what a handful of benchmark repetitions gives — the exact
    permutation distribution is used; otherwise the normal approximation with
    a tie correction and a continuity correction.
    """
    n, m = len(a), len(b)
    if n == 0 or m == 0:
        raise ValueError('Mann-Whitney of an empty sample')
    u_a, ties = rank_sums(a, b)
    has_ties = any(t > 1 for t in ties)

    if not has_ties and n * m <= 400:
        counts = _exact_u_counts(n, m)
        total = sum(counts)
        u = min(u_a, n * m - u_a)
        # Two-sided: both tails at or beyond the observed extremity.
        tail = sum(c for k, c in enumerate(counts) if k <= u or k >= n * m - u)
        p = min(1.0, tail / total)
        return u_a, p, 'exact'

    mu = n * m / 2.0
    n_total = n + m
    tie_term = sum(t ** 3 - t for t in ties)
    var = n * m / 12.0 * ((n_total + 1) - tie_term / (n_total * (n_total - 1.0)))
    if var <= 0:
        return u_a, 1.0, 'degenerate'
    z = (abs(u_a - mu) - 0.5) / math.sqrt(var)
    p = math.erfc(z / math.sqrt(2.0))
    return u_a, min(1.0, p), 'normal'
