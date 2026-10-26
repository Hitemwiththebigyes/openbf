#!/usr/bin/env python3
"""refs.py b1|b2 VA... — every reference to a function address in the ELF: `jal VA` / `j VA` instruction words and the
address as a 4-byte data word (vtables, callbacks). A text search of the decomp misses the last two."""
import os, struct, sys
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..')
P = {'b1': os.path.join(ROOT, 'extracted/bf1/SWBFRONT.ELF'), 'b2': os.path.join(ROOT, 'extracted/bf2/SWBFRNT2.ELF')}
elf = open(P[sys.argv[1]], 'rb').read()
n = len(elf) // 4
ws = struct.unpack('<%dI' % n, elf[:n*4])
for a in sys.argv[2:]:
    va = int(a, 16)
    jal, j = 0x0c000000 | (va >> 2), 0x08000000 | (va >> 2)
    out = {'jal': [], 'j': [], 'data': []}
    for i, w in enumerate(ws):
        if w == jal: out['jal'].append(i*4 - 0x1000 + 0x160000)
        elif w == j: out['j'].append(i*4 - 0x1000 + 0x160000)
        elif w == va: out['data'].append(i*4 - 0x1000 + 0x160000)
    print('%s %#x: jal %s | j %s | data %s' % (sys.argv[1], va, [hex(x) for x in out['jal']], [hex(x) for x in out['j']], [hex(x) for x in out['data']]))
