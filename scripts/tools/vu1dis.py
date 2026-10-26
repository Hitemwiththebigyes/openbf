"""vu1dis.py <bf1|bf2> [lo hi] | find <pattern> -- VU1 main-program disassembler for the SPEC RE (lane 1, 2026-09-24).

Written fresh for this job because the s12/s17 decoder (two earlier local scratch decoders)
has two defects measured here: (1) its loader skips slot 2+257k (k>=1) and keeps the MPG tag slot 2+256k, so the
instruction at every A = 0x100*k (k>=1) is shown as the tag word (e.g. its 0x400 "LQ. vf0,0(vi0) | ADDx. vf16,vf0,vf0"
is {0x00000000, 0x4a000400}); (2) it prints SQ/SQI/SQD as `vf{it}, (vi{is})`, but the VU encoding is
`SQ.dest fs(bits 15-11), imm11(it bits 20-16)` -- base and source are swapped in its output.

Mapping [V, read from the ELF]: `.vutext` = the VIF stream at file off (BF2 0x4935c0 len 0x61c0, BF1 0x3b23c0 len 0x4260);
slot = 8 bytes {lo word, hi word}; slot 1 = {FLUSH, MPG num 0 addr 0}; then {NOP, MPG num 0 addr 0x100*k} at slot 2+257k
(slots 258, 515, 772, ... read with this script's `tags` command). So A (< the main program's end) is at slot 2 + A + A//256.
An instruction slot = lower word (lo, first) and upper word (hi, second) -- the VU's 64-bit instruction little-endian.
"""
import struct, sys
import os

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..') + os.sep  # the repo root
ELF = {'bf2': (ROOT + 'extracted/bf2/SWBFRNT2.ELF', 0x4935c0, 0x61c0),
       'bf1': (ROOT + 'extracted/bf1/SWBFRONT.ELF', 0x3b23c0, 0x4260)}
BC = 'xyzw'


def dest(d):
    return ''.join(c for c, b in zip('xyzw', (8, 4, 2, 1)) if d & b)


def slots(game):
    p, off, size = ELF[game]
    b = open(p, 'rb').read()[off:off + size]
    return [struct.unpack_from('<II', b, i * 8) for i in range(len(b) // 8)]


def main_program(game):
    """A -> (lo, hi) for the program loaded by the leading run of MPG tags (addr 0x000, 0x100, ... contiguous)."""
    s = slots(game)
    prog = {}
    i = 1
    expect = 0
    while i < len(s):
        lo, hi = s[i]
        tag = hi if (hi >> 24) == 0x4a else None
        if tag is None:
            break
        num = (tag >> 16) & 0xff or 256
        addr = tag & 0xffff
        if addr != expect:
            break
        for k in range(num):
            prog[addr + k] = s[i + 1 + k]
        expect = addr + num
        i += 1 + num
    return prog, i


def lower(A, lo):
    op = lo >> 25
    it = (lo >> 16) & 0x1f
    is_ = (lo >> 11) & 0x1f
    id_ = (lo >> 6) & 0x1f
    d = (lo >> 21) & 0xf
    imm11 = lo & 0x7ff
    s11 = imm11 - 0x800 if imm11 & 0x400 else imm11
    imm15 = ((lo >> 21) & 0xf) << 11 | (lo & 0x7ff)
    imm24 = lo & 0xffffff
    if op == 0x00:
        return f'LQ.{dest(d)} vf{it}, {s11}(vi{is_})'
    if op == 0x01:
        return f'SQ.{dest(d)} vf{is_}, {s11}(vi{it})'
    if op == 0x04:
        return f'ILW.{dest(d)} vi{it}, {s11}(vi{is_})'
    if op == 0x05:
        return f'ISW.{dest(d)} vi{it}, {s11}(vi{is_})'
    if op == 0x08:
        return f'IADDIU vi{it}, vi{is_}, {imm15:#x}'
    if op == 0x09:
        return f'ISUBIU vi{it}, vi{is_}, {imm15:#x}'
    fc = {0x10: 'FCEQ', 0x11: 'FCSET', 0x12: 'FCAND', 0x13: 'FCOR'}
    if op in fc:
        return f'{fc[op]} vi1, {imm24:#x}' if op != 0x11 else f'FCSET {imm24:#x}'
    fs = {0x14: 'FSEQ', 0x15: 'FSSET', 0x16: 'FSAND', 0x17: 'FSOR'}
    if op in fs:
        imm12 = ((lo >> 21) & 1) << 11 | (lo & 0x7ff)
        return f'{fs[op]} vi{it}, {imm12:#x}'
    fm = {0x18: 'FMEQ', 0x1a: 'FMAND', 0x1b: 'FMOR'}
    if op in fm:
        return f'{fm[op]} vi{it}, vi{is_}'
    if op == 0x1c:
        return f'FCGET vi{it}'
    if op in (0x20, 0x21):
        t = A + 1 + s11
        return ('B' if op == 0x20 else f'BAL vi{it},') + f' -> {t:#05x}'
    if op == 0x24:
        return f'JR vi{is_}'
    if op == 0x25:
        return f'JALR vi{it}, vi{is_}'
    br = {0x28: 'IBEQ', 0x29: 'IBNE', 0x2c: 'IBLTZ', 0x2d: 'IBGTZ', 0x2e: 'IBLEZ', 0x2f: 'IBGEZ'}
    if op in br:
        t = A + 1 + s11
        if op in (0x28, 0x29):
            return f'{br[op]} vi{it}, vi{is_} -> {t:#05x}'
        return f'{br[op]} vi{is_} -> {t:#05x}'
    if op == 0x40:
        f6 = lo & 0x3f
        sp6 = {0x30: 'IADD', 0x31: 'ISUB', 0x34: 'IAND', 0x35: 'IOR'}
        if f6 in sp6:
            return f'{sp6[f6]} vi{id_}, vi{is_}, vi{it}'
        if f6 == 0x32:
            imm5 = id_ - 32 if id_ & 0x10 else id_
            return f'IADDI vi{it}, vi{is_}, {imm5}'
        f11 = lo & 0x7ff
        fsf = (lo >> 21) & 3
        ftf = (lo >> 23) & 3
        m = {0x33c: 'MOVE', 0x33d: 'MR32'}
        if f11 in m:
            return f'{m[f11]}.{dest(d)} vf{it}, vf{is_}' if d else 'NOP(lower)'
        if f11 == 0x37c:
            return f'LQI.{dest(d)} vf{it}, (vi{is_}++)'
        if f11 == 0x37d:
            return f'SQI.{dest(d)} vf{is_}, (vi{it}++)'
        if f11 == 0x37e:
            return f'LQD.{dest(d)} vf{it}, (--vi{is_})'
        if f11 == 0x37f:
            return f'SQD.{dest(d)} vf{is_}, (--vi{it})'
        if f11 == 0x3bc:
            return f'DIV Q, vf{is_}.{BC[fsf]}, vf{it}.{BC[ftf]}'
        if f11 == 0x3bd:
            return f'SQRT Q, vf{it}.{BC[ftf]}'
        if f11 == 0x3be:
            return f'RSQRT Q, vf{is_}.{BC[fsf]}, vf{it}.{BC[ftf]}'
        if f11 == 0x3bf:
            return 'WAITQ'
        if f11 == 0x3fc:
            return f'MTIR vi{it}, vf{is_}.{BC[fsf]}'
        if f11 == 0x3fd:
            return f'MFIR.{dest(d)} vf{it}, vi{is_}'
        if f11 == 0x3fe:
            return f'ILWR.{dest(d)} vi{it}, (vi{is_})'
        if f11 == 0x3ff:
            return f'ISWR.{dest(d)} vi{it}, (vi{is_})'
        r = {0x43c: 'RNEXT', 0x43d: 'RGET'}
        if f11 in r:
            return f'{r[f11]}.{dest(d)} vf{it}, R'
        if f11 == 0x43e:
            return f'RINIT R, vf{is_}.{BC[fsf]}'
        if f11 == 0x43f:
            return f'RXOR R, vf{is_}.{BC[fsf]}'
        if f11 == 0x67c:
            return f'MFP.{dest(d)} vf{it}, P'
        if f11 == 0x6bc:
            return f'XTOP vi{it}'
        if f11 == 0x6bd:
            return f'XITOP vi{it}'
        if f11 == 0x6fc:
            return f'XGKICK vi{is_}'
        ef = {0x73c: 'ESADD', 0x73d: 'ERSADD', 0x73e: 'ELENG', 0x73f: 'ERLENG', 0x77c: 'EATANxy', 0x77d: 'EATANxz',
              0x77e: 'ESUM', 0x7bc: 'ERCPR', 0x7bd: 'ESQRT', 0x7be: 'ERSQRT', 0x7bf: 'WAITP', 0x7fc: 'ESIN',
              0x7fd: 'EATAN', 0x7fe: 'EEXP'}
        if f11 in ef:
            n = ef[f11]
            if n in ('ESADD', 'ERSADD', 'ELENG', 'ERLENG', 'EATANxy', 'EATANxz', 'ESUM'):
                return f'{n} P, vf{is_}'
            if n == 'WAITP':
                return 'WAITP'
            return f'{n} P, vf{is_}.{BC[fsf]}'
        return f'LOWER? {lo:08x}'
    return f'LOWER? {lo:08x}'


UP_BC = ['ADD', 'SUB', 'MADD', 'MSUB', 'MAX', 'MINI', 'MUL']
UP6 = {0x1c: 'MULq', 0x1d: 'MAXi', 0x1e: 'MULi', 0x1f: 'MINIi', 0x20: 'ADDq', 0x21: 'MADDq', 0x22: 'ADDi',
       0x23: 'MADDi', 0x24: 'SUBq', 0x25: 'MSUBq', 0x26: 'SUBi', 0x27: 'MSUBi', 0x28: 'ADD', 0x29: 'MADD',
       0x2a: 'MUL', 0x2b: 'MAX', 0x2c: 'SUB', 0x2d: 'MSUB', 0x2e: 'OPMSUB', 0x2f: 'MINI'}
UP11 = {0x13c: 'ITOF0', 0x13d: 'ITOF4', 0x13e: 'ITOF12', 0x13f: 'ITOF15', 0x17c: 'FTOI0', 0x17d: 'FTOI4',
        0x17e: 'FTOI12', 0x17f: 'FTOI15', 0x1fc: 'MULAq', 0x1fd: 'ABS', 0x1fe: 'MULAi', 0x1ff: 'CLIP',
        0x23c: 'ADDAq', 0x23d: 'MADDAq', 0x23e: 'ADDAi', 0x23f: 'MADDAi', 0x27c: 'SUBAq', 0x27d: 'MSUBAq',
        0x27e: 'SUBAi', 0x27f: 'MSUBAi', 0x2bc: 'ADDA', 0x2bd: 'MADDA', 0x2be: 'MULA', 0x2fc: 'SUBA',
        0x2fd: 'MSUBA', 0x2fe: 'OPMULA', 0x2ff: 'NOP'}
for base, name in ((0x03c, 'ADDA'), (0x07c, 'SUBA'), (0x0bc, 'MADDA'), (0x0fc, 'MSUBA'), (0x1bc, 'MULA')):
    for k in range(4):
        UP11[base + k] = name + BC[k]


def upper(hi):
    flags = ''.join(f for f, b in (('I', 31), ('E', 30), ('M', 29), ('D', 28), ('T', 27)) if hi >> b & 1)
    d = (hi >> 21) & 0xf
    ft = (hi >> 16) & 0x1f
    fs = (hi >> 11) & 0x1f
    fd = (hi >> 6) & 0x1f
    op6 = hi & 0x3f
    pre = f'[{flags}] ' if flags else ''
    if op6 >= 0x3c:
        f11 = hi & 0x7ff
        n = UP11.get(f11, f'UP11?{f11:#x}')
        if n == 'NOP':
            return pre + 'NOP'
        if n in ('ITOF0', 'ITOF4', 'ITOF12', 'ITOF15', 'FTOI0', 'FTOI4', 'FTOI12', 'FTOI15', 'ABS'):
            return pre + f'{n}.{dest(d)} vf{ft}, vf{fs}'
        if n == 'CLIP':
            return pre + f'CLIPw.xyz vf{fs}, vf{ft}.w'
        if n[-1] in 'xyzw' and n[:-1] in ('ADDA', 'SUBA', 'MADDA', 'MSUBA', 'MULA'):
            return pre + f'{n}.{dest(d)} ACC, vf{fs}, vf{ft}.{n[-1]}'
        if n.endswith('q') or n.endswith('i'):
            return pre + f'{n}.{dest(d)} ACC, vf{fs}, {n[-1].upper()}'
        return pre + f'{n}.{dest(d)} ACC, vf{fs}, vf{ft}'
    if op6 < 0x1c:
        n = UP_BC[op6 >> 2]
        return pre + f'{n}{BC[op6 & 3]}.{dest(d)} vf{fd}, vf{fs}, vf{ft}.{BC[op6 & 3]}'
    n = UP6.get(op6, f'UP6?{op6:#x}')
    if n.endswith('q') or n.endswith('i'):
        return pre + f'{n}.{dest(d)} vf{fd}, vf{fs}, {n[-1].upper()}'
    return pre + f'{n}.{dest(d)} vf{fd}, vf{fs}, vf{ft}'


def line(A, lo, hi):
    hi_s = upper(hi)
    lo_s = f'LOI {lo:#010x} ({struct.unpack("<f", struct.pack("<I", lo))[0]:g})' if hi >> 31 & 1 else lower(A, lo)
    return f'{A:#05x}: {lo_s:40s} | {hi_s}'


if __name__ == '__main__':
    g = sys.argv[1]
    prog, end = main_program(g)
    if len(sys.argv) > 2 and sys.argv[2] == 'tags':
        for i, (lo, hi) in enumerate(slots(g)):
            if (hi >> 24) == 0x4a and (lo == 0 or lo == 0x11000000):
                print('slot %d: %08x %08x  MPG num %d addr %#x' % (i, lo, hi, (hi >> 16) & 0xff or 256, hi & 0xffff))
        print('main program: A 0..%#x, ends before slot %d' % (max(prog), end))
    elif len(sys.argv) > 2 and sys.argv[2] == 'find':
        import re
        pat = re.compile(sys.argv[3])
        for A in sorted(prog):
            s = line(A, *prog[A])
            if pat.search(s):
                print(s)
    else:
        lo_a = int(sys.argv[2], 16) if len(sys.argv) > 2 else 0
        hi_a = int(sys.argv[3], 16) if len(sys.argv) > 3 else max(prog) + 1
        for A in range(lo_a, hi_a):
            if A in prog:
                print(line(A, *prog[A]))
