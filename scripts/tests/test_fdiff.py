"""FDIFF contract tests: known divergent fields, tolerance and unusable inputs."""
import contextlib
import copy
import io
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import fdiff


# Synthetic integers use the SHASH on-wire layout; this hash encoder is
# independent of the differ's decoder/hash implementation.
BASE = {
    'units': [1, 25041, 7, 208, -16, 0, 400, 0, 1],
    'rng': [42], 'score': [250, 0, 0, 250, 0, 0],
    'posts': [1, 0, 0, 255, 0, 0],
    'timers': [1, 4294967295, 255, 255, 0, 0, 0, 1, 25041, 7, 18446744073709551615],
}
TYPES = {
    'units': [4, 4, 4, -4, -4, -4, -4, 1, 1], 'rng': [4],
    'score': [4, 8, 8, 4, 8, 8], 'posts': [4, 4, 1, 1, -4, -4],
    'timers': [8, 4, 1, 1, -4, -4, -4, 4, 4, 4, 8],
}


def encode(group, values):
    result = 14695981039346656037
    for value, width in zip(values, TYPES[group]):
        for byte in value.to_bytes(abs(width), 'little', signed=width < 0):
            result = (result ^ byte) * 1099511628211 % 2**64
    return f'{result:016x}'


def trace(rows=None, full=True, quantum='0.0625', unavailable=()):
    rows = rows if rows is not None else [copy.deepcopy(BASE) for _ in range(4)]
    head = ['# openbf-trace 1', '# map fixture', '# seed 42', '# tick_hz 30',
            f'# quanta position={quantum} health=0.25 capture=0.00390625 timer=0.0009765625',
            '# encoding ' + fdiff.ENCODING]
    head += [f'# {group} {fdiff.SCHEMAS[group]}' for group in fdiff.GROUPS]
    head += ['# columns t units rng score posts timers', '# full ' + str(full).lower()]
    for tick, row in enumerate(rows, 1):
        hashes = {g: 'n/a' if g in unavailable else encode(g, row[g]) for g in fdiff.GROUPS}
        head.append(str(tick) + ' ' + ' '.join(hashes.values()))
        if full:
            head += [f'# full {tick} {g} ' + ' '.join(map(str, row[g])) for g in fdiff.GROUPS if g not in unavailable]
    return '\n'.join(head) + '\n'


class DifferTests(unittest.TestCase):
    def run_diff(self, a, b, *args):
        with tempfile.TemporaryDirectory() as directory:
            pa, pb = Path(directory) / 'a', Path(directory) / 'b'
            pa.write_text(a)
            pb.write_text(b)
            out = io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(out):
                rc = fdiff.main([str(pa), str(pb), *args])
            return rc, out.getvalue()

    def changed(self, group='units', index=3, value=210):
        rows = [copy.deepcopy(BASE) for _ in range(4)]
        rows[2][group][index] = value
        return trace(rows)

    def test_known_divergence(self):
        rc, out = self.run_diff(trace(), self.changed())
        self.assertEqual(rc, 1, out)
        self.assertIn('tick 3, group units, unit 0x61d1, field x: A=13.0000 B=13.1250', out)
        self.assertIn('previous matching tick 2', out)
        self.assertIn('fdiff: matched 2 of 4 ticks first_divergence tick 3 group units field x', out)

    def test_equal_compact_and_full(self):
        for full in (True, False):
            rc, out = self.run_diff(trace(full=full), trace(full=full))
            self.assertEqual(rc, 0, out)
            self.assertIn('matched 4 of 4 ticks first_divergence none', out)

    def test_every_group_and_late_field(self):
        for group, index, value, field in (
            ('units', 6, 399, 'health'), ('rng', 0, 43, 'lcg'),
            ('score', 5, 1, 'casualties'), ('posts', 5, 1, 'neutral'),
            ('timers', 10, 300, 'respawn_at_tick'), ('units', 2, 8, 'gen'),
            ('units', 1, 25042, 'id')):
            with self.subTest(group=group, field=field):
                rc, out = self.run_diff(trace(), self.changed(group, index, value))
                self.assertEqual(rc, 1, out)
                self.assertIn(f'group {group} field {field}', out)

    def test_tolerance_boundary_and_qualification(self):
        for option in ('x=0.125', 'units.x=0.125'):
            self.assertEqual(self.run_diff(trace(), self.changed(), '--tol', option)[0], 0)
        self.assertEqual(self.run_diff(trace(), self.changed(), '--tol', 'x=0.124')[0], 1)
        self.assertEqual(self.run_diff(trace(), self.changed(), '--tol', 'health=100')[0], 1)

    def test_groups_and_order(self):
        self.assertEqual(self.run_diff(trace(), self.changed(), '--groups', 'rng')[0], 0)
        rows = [copy.deepcopy(BASE) for _ in range(4)]
        rows[2]['units'][3] += 2
        rows[2]['rng'][0] += 1
        rc, out = self.run_diff(trace(), trace(rows), '--groups', 'rng,units')
        self.assertEqual(rc, 1)
        self.assertIn('group units field x', out)

    def test_header_mismatch(self):
        for old, new in (('# tick_hz 30', '# tick_hz 60'), ('# map fixture', '# map other'),
                         ('# seed 42', '# seed 43'), ('# openbf-trace 1', '# openbf-trace 2'),
                         ('x:i32', 'x:i64'), ('position=0.0625', 'position=0.125')):
            self.assertEqual(self.run_diff(trace(), trace().replace(old, new))[0], 2, new)

    def test_coarser_quantum(self):
        rows = [copy.deepcopy(BASE) for _ in range(4)]
        for row in rows:
            row['units'][3] = 104
            row['units'][4] = -8
        a, b = trace(), trace(rows, quantum='0.125')
        self.assertEqual(self.run_diff(a, b, '--coarser-quanta')[0], 0)
        # Both positive and negative ties round away from zero.
        rows[0]['units'][3] = 105
        arows = [copy.deepcopy(BASE) for _ in range(4)]
        arows[0]['units'][3] = 209
        self.assertEqual(self.run_diff(trace(arows), trace(rows, quantum='0.125'), '--coarser-quanta')[0], 0)
        # Negative ties have the same away-from-zero rule.
        arows[0]['units'][4] = -17
        rows[0]['units'][4] = -9
        self.assertEqual(self.run_diff(trace(arows), trace(rows, quantum='0.125'), '--coarser-quanta')[0], 0)
        rows[0]['units'][3] = 104
        self.assertEqual(self.run_diff(trace(arows), trace(rows, quantum='0.125'), '--coarser-quanta')[0], 1)

    def test_compact_hash_diagnostic(self):
        rc, out = self.run_diff(trace(full=False), trace(full=False).replace(encode('units', BASE['units']), '0123456789abcdef'))
        self.assertEqual(rc, 1, out)
        self.assertIn('field hash', out)
        self.assertIn('--trace-full', out)
        self.assertIn('previous matching tick none', out)

    def test_missing_raw_for_tolerance(self):
        self.assertEqual(self.run_diff(trace(full=False), trace(full=False), '--tol', 'x=1')[0], 2)

    def test_unavailable(self):
        rc, out = self.run_diff(trace(), trace(unavailable=('rng',)))
        self.assertEqual(rc, 0, out)
        self.assertIn('skipped unavailable groups rng', out)
        self.assertEqual(self.run_diff(trace(), trace(unavailable=('rng',)), '--groups', 'rng')[0], 2)

    def test_bad_input(self):
        good = trace()
        for bad in ('', good.replace('# map fixture\n', ''), good.replace('# full 2 rng 42', '# full 2 rng 43'),
                    good.replace('# full 3 rng 42\n', ''), good.replace('3 ', '2 ', 1),
                    good.replace('# full 2 units 1 ', '# full 2 units 2 '),
                    good.replace('# full 2 rng 42', '# full 2 rng -1'),
                    good + 'garbage\n', good.replace('# seed 42', '# seed NaN'),
                    good.replace('position=0.0625', 'position=NaN')):
            with self.subTest(bad=bad[:90]):
                self.assertEqual(self.run_diff(good, bad)[0], 2)

    def test_invalid_options(self):
        for args in (('--tol', 'id=1'), ('--tol', 'x=-1'), ('--tol', 'x=NaN'),
                     ('--tol', 'x=Infinity'), ('--groups', 'unit'), ('--groups', 'rng,rng'),
                     ('--tol', 'x=1', '--tol', 'x=2')):
            self.assertEqual(self.run_diff(trace(), trace(), *args)[0], 2)

    def test_missing_sample_reports_earliest_tick_in_both_orders(self):
        ordinary = trace()
        shifted = ordinary.replace('\n3 ', '\n5 ').replace('\n4 ', '\n6 ').replace('# full 3 ', '# full 5 ').replace('# full 4 ', '# full 6 ')
        for a, b in ((ordinary, shifted), (shifted, ordinary)):
            rc, out = self.run_diff(a, b)
            self.assertEqual(rc, 1, out)
            self.assertIn('first_divergence tick 3 group trace field tick', out)

    def test_tick_and_length_mismatch(self):
        for a, b in ((trace(), trace([BASE])), (trace([BASE]), trace()),
                     (trace(), trace().replace('\n3 ', '\n5 ').replace('\n4 ', '\n6 ').replace('# full 3 ', '# full 5 ').replace('# full 4 ', '# full 6 '))):
            rc, out = self.run_diff(a, b)
            self.assertEqual(rc, 1, out)
            self.assertIn('group trace field tick', out)


if __name__ == '__main__':
    unittest.main()
