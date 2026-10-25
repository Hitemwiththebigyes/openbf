#!/usr/bin/env python3
"""Compare openbf-trace 1 streams. Exit 0: match, 1: divergence, 2: bad input.

Tolerances are absolute world units (metres, health points, seconds), not
quantized integers. Only measured float fields accept tolerances. This is a
selected-state comparison, not proof that all future-affecting state agrees.
"""
import argparse
from contextlib import ExitStack
from dataclasses import dataclass
from decimal import Decimal, InvalidOperation, ROUND_HALF_UP
from itertools import zip_longest
from pathlib import Path
import re
import sys

GROUPS = ('units', 'rng', 'score', 'posts', 'timers')
SCHEMAS = {
    'units': 'count:u32 (id:u32 gen:u32 x:i32 y:i32 z:i32 health:i32 team:u8 alive:u8)*',
    'rng': 'lcg:u32',
    'score': '(tickets:u32 kills:u64 casualties:u64)*2 teams=0,1',
    'posts': 'count:u32 (id:u32 owner:u8 capture_team:u8 capture:i32 neutral:i32)*',
    'timers': 'elapsed_ticks:u64 limit_seconds:u32 outcome:u8 domination_team:u8 domination:i32 wave0:i32 wave1:i32 count:u32 (id:u32 gen:u32 respawn_at_tick:u64)*',
}
ENCODING = ('fnv1a64 little-endian round-nearest-ties-away none_u8=255 '
            'none_u32=4294967295 none_u64=18446744073709551615')
QUANTA = {'x': 'position', 'y': 'position', 'z': 'position', 'health': 'health',
          'capture': 'capture', 'neutral': 'capture', 'domination': 'timer',
          'wave0': 'timer', 'wave1': 'timer'}
FLOAT_FIELDS = {f'{g}.{f}' for g, fs in (
    ('units', ('x', 'y', 'z', 'health')), ('posts', ('capture', 'neutral')),
    ('timers', ('domination', 'wave0', 'wave1'))) for f in fs}


class InvalidTrace(ValueError):
    pass


def positive_decimal(text, zero=False):
    try:
        value = Decimal(text)
    except InvalidOperation:
        raise InvalidTrace(f'invalid decimal: {text}') from None
    if not value.is_finite() or value < 0 or (not zero and value == 0):
        raise InvalidTrace(f'invalid {"nonnegative" if zero else "positive"} decimal: {text}')
    # Keep Decimal arithmetic exact for supported trace precision.
    if abs(value.adjusted()) > 12 or len(value.as_tuple().digits) > 12:
        raise InvalidTrace(f'decimal precision/range exceeds trace support: {text}')
    return value


@dataclass
class Field:
    entity: str
    name: str
    value: int
    kind: str


def decode(group, values):
    """Decode and bounds-check the canonical flattened tuple, including counts."""
    fields = []
    pos = 0

    def take(name, kind, entity='-'):
        nonlocal pos
        if pos == len(values):
            raise InvalidTrace(f'truncated {group} tuple at {name}')
        value = values[pos]
        pos += 1
        bits = int(kind[1:])
        low, high = (-(1 << (bits - 1)), (1 << (bits - 1)) - 1) if kind[0] == 'i' else (0, (1 << bits) - 1)
        if not low <= value <= high:
            raise InvalidTrace(f'{group}.{name} outside {kind}')
        fields.append(Field(entity, name, value, kind))
        return value

    if group in ('units', 'posts'):
        count = take('count', 'u32')
        width = 8 if group == 'units' else 5
        if count * width != len(values) - pos:
            raise InvalidTrace(f'{group} count does not match tuple length')
        previous = -1
        for _ in range(count):
            ident = take('id', 'u32')
            if ident <= previous:
                raise InvalidTrace(f'{group} ids must be strictly increasing')
            previous = ident
            entity = f'unit 0x{ident:x}' if group == 'units' else f'post {ident}'
            fields[-1].entity = entity
            specs = ('gen:u32 x:i32 y:i32 z:i32 health:i32 team:u8 alive:u8' if group == 'units'
                     else 'owner:u8 capture_team:u8 capture:i32 neutral:i32')
            for spec in specs.split():
                take(*spec.split(':'), entity)
    elif group == 'rng':
        take('lcg', 'u32')
    elif group == 'score':
        for team in range(2):
            for name, kind in (('tickets', 'u32'), ('kills', 'u64'), ('casualties', 'u64')):
                take(name, kind, f'team {team}')
    else:
        for spec in SCHEMAS['timers'].split(' count:')[0].split():
            take(*spec.split(':'))
        count = take('count', 'u32')
        if count * 3 != len(values) - pos:
            raise InvalidTrace('timers count does not match tuple length')
        previous = -1
        for _ in range(count):
            ident = take('id', 'u32')
            if ident <= previous:
                raise InvalidTrace('timers ids must be strictly increasing')
            previous = ident
            entity = f'unit 0x{ident:x}'
            fields[-1].entity = entity
            take('gen', 'u32', entity)
            take('respawn_at_tick', 'u64', entity)
    if pos != len(values):
        raise InvalidTrace(f'extra values in {group} tuple')
    return fields


def tuple_hash(fields):
    value = 0xcbf29ce484222325
    for field in fields:
        for byte in field.value.to_bytes(int(field.kind[1:]) // 8, 'little', signed=field.kind[0] == 'i'):
            value = ((value ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    return f'{value:016x}'


class Trace:
    def __init__(self, file):
        self.file = file
        self.line = 0
        self.pending = None
        self.header = {}
        while True:
            line = self.read()
            if line is None:
                break
            if not line.startswith('# '):
                self.pending = line
                break
            parts = line[2:].split(' ', 1)
            if len(parts) != 2 or parts[0] in self.header:
                raise InvalidTrace('malformed or duplicate header')
            self.header[parts[0]] = parts[1]
        expected = dict(SCHEMAS, **{'openbf-trace': '1', 'encoding': ENCODING,
                                    'columns': 't ' + ' '.join(GROUPS)})
        for key, value in expected.items():
            if self.header.get(key) != value:
                raise InvalidTrace(f'unsupported/missing header {key}')
        for key in ('map', 'seed', 'tick_hz', 'quanta', 'full'):
            if key not in self.header:
                raise InvalidTrace(f'missing header {key}')
        if self.header['full'] not in ('true', 'false'):
            raise InvalidTrace('full must be true or false')
        if not re.fullmatch(r'\d+', self.header['seed']) or int(self.header['seed']) > 0xffffffff:
            raise InvalidTrace('invalid seed')
        if not re.fullmatch(r'[1-9]\d*', self.header['tick_hz']):
            raise InvalidTrace('invalid tick_hz')
        pairs = [token.split('=') for token in self.header['quanta'].split()]
        if any(len(pair) != 2 for pair in pairs) or len(pairs) != 4:
            raise InvalidTrace('invalid quanta')
        self.quanta = {k: positive_decimal(v) for k, v in pairs}
        if set(self.quanta) != set(QUANTA.values()):
            raise InvalidTrace('missing/duplicate/unknown quantum')

    def read(self):
        while True:
            line = self.file.readline()
            self.line += 1
            if not line:
                return None
            line = line.strip()
            if line:
                return line

    def records(self):
        previous = -1
        line = self.pending
        while line is not None:
            parts = line.split()
            if len(parts) != 6 or not re.fullmatch(r'\d+', parts[0]):
                raise InvalidTrace(f'invalid tick record at line {self.line}')
            tick = int(parts[0])
            if tick <= previous or tick > 0xffffffffffffffff:
                raise InvalidTrace('ticks must be strictly increasing u64')
            previous = tick
            hashes = dict(zip(GROUPS, parts[1:]))
            if any(not re.fullmatch(r'[0-9a-f]{16}|n/a', h) for h in hashes.values()):
                raise InvalidTrace(f'invalid hash at tick {tick}')
            raw = {}
            line = self.read()
            while line is not None and line.startswith('# full '):
                row = line.split()
                if len(row) < 5 or row[2] != str(tick) or row[3] not in GROUPS or row[3] in raw:
                    raise InvalidTrace(f'invalid/duplicate full row at tick {tick}')
                group = row[3]
                if not all(re.fullmatch(r'-?\d+', v) for v in row[4:]):
                    raise InvalidTrace('non-integer tuple')
                fields = decode(group, [int(v) for v in row[4:]])
                if tuple_hash(fields) != hashes[group]:
                    raise InvalidTrace(f'full tuple/hash mismatch at tick {tick} group {group}')
                raw[group] = fields
                line = self.read()
            if self.header['full'] == 'true' and set(raw) != {g for g in GROUPS if hashes[g] != 'n/a'}:
                raise InvalidTrace(f'missing full tuple at tick {tick}')
            yield tick, hashes, raw


def difference(group, left, right, qa, qb, tolerances, coarser):
    for a, b in zip_longest(left, right):
        if a is None or b is None:
            return '-', 'tuple_length', str(len(left)), str(len(right))
        # Count/id differences are reported before later row identities.
        if a.name != b.name:
            return a.entity, 'field_layout', a.name, b.name
        if a.entity != b.entity and a.name != 'id':
            return a.entity, 'identity', a.entity, b.entity
        av, bv = a.value, b.value
        if f'{group}.{a.name}' in FLOAT_FIELDS:
            quantum = QUANTA[a.name]
            av, bv = Decimal(av) * qa[quantum], Decimal(bv) * qb[quantum]
            if coarser:
                q = max(qa[quantum], qb[quantum])
                av = (av / q).to_integral_value(rounding=ROUND_HALF_UP) * q
                bv = (bv / q).to_integral_value(rounding=ROUND_HALF_UP) * q
            eps = tolerances.get(f'{group}.{a.name}', tolerances.get(a.name, Decimal(0)))
            if abs(av - bv) <= eps:
                continue
        elif av == bv:
            continue
        return a.entity, a.name, str(av), str(bv)
    return None


def compare(a, b, groups, tolerances, coarser):
    for key in ('map', 'seed', 'tick_hz'):
        if a.header[key] != b.header[key]:
            raise InvalidTrace(f'header mismatch {key}: A={a.header[key]} B={b.header[key]}')
    if a.quanta != b.quanta and not coarser:
        raise InvalidTrace('quanta mismatch; use --coarser-quanta with full traces')
    raw_mode = bool(tolerances) or (coarser and a.quanta != b.quanta)
    first, last, matched, total = None, None, 0, 0
    skipped = set()
    for left, right in zip_longest(a.records(), b.records()):
        total += 1
        diff = None
        tick = min(row[0] for row in (left, right) if row is not None)
        if left is None or right is None or left[0] != right[0]:
            diff = ('trace', '-', 'tick', str(left[0]) if left else '<end>', str(right[0]) if right else '<end>')
        else:
            for group in groups:
                ha, hb = left[1][group], right[1][group]
                if 'n/a' in (ha, hb):
                    skipped.add(group)
                    continue
                if raw_mode and (group not in left[2] or group not in right[2]):
                    raise InvalidTrace(f'tick {tick} group {group}: raw comparison requires --trace-full on both inputs')
                detail = None
                if raw_mode:
                    detail = difference(group, left[2][group], right[2][group], a.quanta, b.quanta, tolerances, coarser)
                elif ha != hb:
                    if group in left[2] and group in right[2]:
                        detail = difference(group, left[2][group], right[2][group], a.quanta, b.quanta, {}, False)
                    if detail is None:
                        detail = ('-', 'hash', ha, hb)
                if detail is not None and diff is None:
                    diff = (group, *detail)
            if all('n/a' in (left[1][g], right[1][g]) for g in groups):
                raise InvalidTrace(f'no comparable selected groups at tick {tick}')
        if first is None:
            if diff:
                first = (tick, *diff)
            else:
                matched += 1
                last = tick
    if total == 0:
        raise InvalidTrace('empty traces have no comparable ticks')
    if skipped:
        print('fdiff: skipped unavailable groups ' + ','.join(sorted(skipped)))
    if first:
        tick, group, entity, field, av, bv = first
        print(f'first mismatch: tick {tick}, group {group}, {entity}, field {field}: A={av} B={bv}; previous matching tick {last if last is not None else "none"}')
        if field == 'hash':
            print('Re-run both producers with --trace-full to identify the unit and field.')
        print(f'fdiff: matched {matched} of {total} ticks first_divergence tick {tick} group {group} field {field}')
        return 1
    print(f'fdiff: matched {matched} of {total} ticks first_divergence none')
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('A', nargs='?')
    parser.add_argument('B', nargs='?')
    parser.add_argument('--groups', default=','.join(GROUPS))
    parser.add_argument('--tol', action='append', default=[], metavar='FIELD=EPS')
    parser.add_argument('--coarser-quanta', action='store_true', help='compare full tuples at the coarser quantum (round ties away from zero)')
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args(argv)
    if args.self_test:
        import unittest
        suite = unittest.defaultTestLoader.discover(str(Path(__file__).parent / 'tests'), pattern='test_fdiff.py')
        return 0 if unittest.TextTestRunner().run(suite).wasSuccessful() else 1
    if not args.A or not args.B:
        parser.error('A and B are required')
    try:
        groups = args.groups.split(',')
        if not groups or len(set(groups)) != len(groups) or not set(groups) <= set(GROUPS):
            raise InvalidTrace('groups must be distinct names from ' + ','.join(GROUPS))
        groups = [g for g in GROUPS if g in groups]
        tolerances = {}
        for spec in args.tol:
            field, sep, value = spec.partition('=')
            if not sep or field in tolerances or field not in FLOAT_FIELDS | set(QUANTA):
                raise InvalidTrace(f'invalid/duplicate tolerance field: {field}')
            tolerances[field] = positive_decimal(value, zero=True)
        with ExitStack() as stack:
            traces = [Trace(stack.enter_context(open(p, encoding='utf-8'))) for p in (args.A, args.B)]
            return compare(*traces, groups, tolerances, args.coarser_quanta)
    except (InvalidTrace, OSError, UnicodeError, ValueError) as error:
        print(f'fdiff: unusable input: {error}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
