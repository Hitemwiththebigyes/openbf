#!/usr/bin/env python3
"""Annotate name-hash literals without changing decomp line numbers.

Usage: annotate_hashes.py --names PATH [--names PATH ...] [--root CHECKOUT]
PATH is a text name list or a directory of text sources (ODFs, strings, property
 tables). ASCII identifier/path tokens are candidates, not verified property
names. Supply all restored name sources for coverage. Outputs both
analysis/<ELF>-decomp.annotated.c plus provenance JSON; originals are untouched.
Comments label candidate names only: a numeric match does not prove a reader.
Ghidra folds lui/addiu pairs into one literal, but prints a hash it mistakes for
an address as &DAT_<hex> / &UNK_<hex>; those are the 'pointer' form (only above
PS2 main RAM, so real data addresses are never annotated).
"""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path
import re

CORPORA = ('SWBFRONT.ELF', 'SWBFRNT2.ELF')
FORMS = ('hex', 'negative_hex', 'unsigned_decimal', 'signed_decimal', 'sign_extended',
         'pointer')
# Above the 32 MiB EE main RAM: an &DAT_/&UNK_ there is a mistyped constant.
POINTER_MIN = 0x02000000
# Common sentinels, IEEE float words, FNV arithmetic constants and test patterns.
NON_HASH_WORDS = frozenset((0xffffffff, 0x7fffffff, 0x80000000, 0x3f800000,
                            0xbf800000, 0x40000000, 0x40400000, 0x3f000000,
                            0x811c9dc5, 0x01000193, 0xdeadbeef, 0xcccccccc))
NAME = re.compile(r'[A-Za-z_][A-Za-z0-9_./\\:@-]*')
MARKER = re.compile(r'/\* FNV: (\[[^\n]*?\]) \*/')
# Consume strings, comments, identifiers and floats whole so their embedded
# digits cannot be mistaken for integer literals. Keep the original spelling.
TOKEN = re.compile(
    r'(?P<pointer>&(?:DAT|UNK)_(?P<address>[0-9a-fA-F]{8}))(?!\w)|'
    r'(?P<skip>/\*.*?\*/|//[^\n]*|"(?:\\.|[^"\\])*"|'
    r"'(?:\\.|[^'\\])*'|[A-Za-z_]\w*|"
    r'(?:\d+\.\d*|\.\d+)(?:[eE][+-]?\d+)?[fFlL]?|\d+[eE][+-]?\d+[fFlL]?)'
    r'|(?P<number>-?(?:0[xX][0-9a-fA-F]+|[0-9]+)(?:[uUlL]*))(?![\w.])', re.S)


def fnv(name):
    """Same byte OR 0x20 operation as mungefs::fnv1a_lowercase."""
    value = 2166136261
    for byte in name.encode('utf-8'):
        value = ((value ^ (byte | 0x20)) * 16777619) & 0xffffffff
    return value


def name_table(names):
    buckets = defaultdict(set)
    for name in names:
        if not NAME.fullmatch(name):
            raise ValueError(f'invalid candidate name: {name!r}')
        buckets[fnv(name)].add(name)
    # Case aliases have the same input bytes to FNV, and are not collisions.
    collisions = [ns for ns in buckets.values()
                  if len({bytes(b | 0x20 for b in n.encode()) for n in ns}) > 1]
    return {h: sorted(ns) for h, ns in buckets.items()}, {
        'collision_buckets': len(collisions), 'colliding_names': sum(map(len, collisions))}


def literal(token):
    raw = token.rstrip('uUlL')
    negative = raw.startswith('-')
    base = 16 if raw.lstrip('-').lower().startswith('0x') else 10
    if base == 10 and len(raw.lstrip('-')) > 1 and raw.lstrip('-').startswith('0'):
        return None  # C octal is not decimal.
    value = int(raw, base)
    if value < -0x80000000 or value > 0xffffffffffffffff:
        return None
    if value > 0xffffffff:
        if base != 16 or value >> 32 != 0xffffffff or not value & 0x80000000:
            return None
        form = 'sign_extended'
    else:
        form = ('negative_hex' if negative else 'hex') if base == 16 else (
            'signed_decimal' if negative else 'unsigned_decimal')
    return value & 0xffffffff, form


def annotate(text, table, excluded=NON_HASH_WORDS):
    counts = Counter({form: 0 for form in FORMS})
    def replace(match):
        if match.group('pointer'):
            value, form = int(match.group('address'), 16), 'pointer'
            if value < POINTER_MIN:
                return match.group()
        elif not match.group('number'):
            return match.group()
        else:
            result = literal(match.group())
            if result is None:
                return match.group()
            value, form = result
        if value < 0x10000 or value in excluded or value not in table:
            return match.group()
        counts[form] += 1
        return match.group() + '/* FNV: ' + json.dumps(table[value], ensure_ascii=True) + ' */'
    return TOKEN.sub(replace, text), dict(counts)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def annotated_path(source):
    return source.with_name(source.name.replace('-decomp.c', '-decomp.annotated.c'))


def write_corpus(source, table, excluded=NON_HASH_WORDS):
    original = source.read_bytes()
    output, counts = annotate(original.decode('utf-8'), table, excluded)
    data = output.encode('utf-8')
    target = annotated_path(source)
    target.write_bytes(data)
    target.with_suffix('.json').write_text(json.dumps({
        'version': 1, 'source_sha256': digest(original), 'annotated_sha256': digest(data),
        'names_sha256': digest(json.dumps(table, sort_keys=True).encode()),
        'excluded_words': sorted(excluded), 'counts': counts}, indent=2) + '\n')
    return counts


def read_annotated(source):
    target = annotated_path(source)
    metadata = json.loads(target.with_suffix('.json').read_text())
    data = target.read_bytes()
    if (metadata.get('version') != 1 or metadata['source_sha256'] != digest(source.read_bytes())
            or metadata['annotated_sha256'] != digest(data)):
        raise ValueError(f'stale or modified annotation: regenerate {target}')
    return data.decode('utf-8').split('\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--names', type=Path, action='append', required=True)
    parser.add_argument('--exclude-word', type=lambda x: int(x, 0), action='append', default=[])
    args = parser.parse_args()
    try:
        names = set()
        for path in args.names:
            if not path.exists():
                raise OSError(f'name source unavailable: {path}')
            files = sorted(p for p in path.rglob('*') if p.is_file()) if path.is_dir() else [path]
            for file in files:
                # A binary needs its format-specific name extractor; do not mine
                # arbitrary binary bytes into plausible property names.
                data = file.read_bytes()
                if b'\0' in data:
                    raise ValueError(f'binary name source needs a text export: {file}')
                names.update(NAME.findall(data.decode('utf-8')))
        if not names:
            raise ValueError('no candidate names found')
        sources = [args.root / 'analysis' / f'{elf}-decomp.c' for elf in CORPORA]
        for source in sources:
            if not source.is_file():
                raise OSError(f'corpus unavailable: {source}')
        table, collisions = name_table(names)
        print(f'{len(names)} candidate names; collisions: {json.dumps(collisions, sort_keys=True)}')
        for source in sources:
            counts = write_corpus(source, table, NON_HASH_WORDS | set(args.exclude_word))
            print(f'{annotated_path(source)}: {json.dumps(counts, sort_keys=True)}')
    except (OSError, ValueError) as exc:
        parser.exit(2, f'{exc}\n')


if __name__ == '__main__':
    main()
