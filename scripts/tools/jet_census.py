#!/usr/bin/env python3
"""jet_census.py <bf1|bf2> -- every soldier class (entc) on the disc that authors a jet key, with its jet and movement
keys, read straight from the LVLs under extracted/<game>/lvl (card JETP, analysis/re-jetpack.md 2.2).

Keys are named by FNV-1a over the lowercased name (the munge hash). Identical class chunks found in several LVLs are
printed once with the LVL list. BASE is printed; inherited keys are not resolved (the BF2 bases are in INGAME.LVL /
the side LVLs, and the baked classes/*.ron hold them)."""
import glob
import os
import struct
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..')


def fnv(name):
    v = 2166136261
    for b in name.encode():
        v = ((v ^ (b | 0x20)) * 16777619) & 0xffffffff
    return v


JET = """JetJump JetPush JetAcceleration JetType JetFuelCost JetFuelInitialCost JetFuelRechargeRate JetFuelMinBorder
JetEnergyDrain JetDuration JetEffect""".split()
OTHER = """EngineSound TurnOnSound TurnOffSound ControlSpeed MaxSpeed MaxStrafeSpeed JumpHeight Acceleration Acceleraton
GeometryName""".split()
NAMES = {fnv(k): k for k in JET + OTHER}
JETH = {fnv(k) for k in JET}


def walk(d, off, end):
    while off + 8 <= end:
        tag = d[off:off + 4]
        size = struct.unpack('<I', d[off + 4:off + 8])[0]
        yield tag, off + 8, size
        if tag in (b'ucfb', b'lvl_'):
            yield from walk(d, off + 8 + (8 if tag == b'lvl_' else 0), off + 8 + size)
        off = (off + 8 + size + 3) & ~3


def classes(path):
    d = open(path, 'rb').read()
    for tag, o, sz in walk(d, 0, len(d)):
        if tag != b'entc':
            continue
        base = typ = None
        props = []
        p = o
        while p + 8 <= o + sz:
            t = d[p:p + 4]
            s = struct.unpack('<I', d[p + 4:p + 8])[0]
            b = d[p + 8:p + 8 + s]
            if t == b'BASE':
                base = b.split(b'\0')[0].decode('latin1')
            elif t == b'TYPE':
                typ = b.split(b'\0')[0].decode('latin1')
            elif t == b'PROP':
                props.append((struct.unpack('<I', b[:4])[0], b[4:].split(b'\0')[0].decode('latin1')))
            p = (p + 8 + s + 3) & ~3
        yield base, typ, props


def main():
    game = sys.argv[1]
    root = os.path.join(ROOT, 'extracted', game, 'lvl')
    seen = {}
    for f in sorted(glob.glob(root + '/**/*.LVL', recursive=True)):
        for base, typ, props in classes(f):
            if any(h in JETH for h, _ in props):
                seen.setdefault((typ or '', base, tuple(props)), []).append(os.path.relpath(f, root))
    for (typ, base, props), fs in sorted(seen.items()):
        print(f"== {typ} (BASE {base}) in {', '.join(sorted(set(fs)))}")
        for h, v in props:
            if h in NAMES:
                print(f"    {NAMES[h]} = {v}")


if __name__ == '__main__':
    main()
