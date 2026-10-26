"""eedis2.py <bf1|bf2> <va_lo> <va_hi> -- eedis.py plus the unaligned loads/stores (lwl/lwr/swl/swr/ldl/ldr/sdl/sdr),
lwu, blezl/bgtzl, bc1t/bc1f, cfc1/ctc1, COP2 bc2/vcallms, and the jal target's name if it's a known label."""
import os, struct, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import eedis
R = eedis.R
EXTRA = {0x22: 'lwl', 0x26: 'lwr', 0x2a: 'swl', 0x2e: 'swr', 0x1a: 'ldl', 0x1b: 'ldr', 0x2c: 'sdl', 0x2d: 'sdr',
         0x27: 'lwu', 0x37: 'ld', 0x3f: 'sd'}


def dis(w, va):
    op = w >> 26
    rs, rt, rd, sa, fn = (w >> 21) & 31, (w >> 16) & 31, (w >> 11) & 31, (w >> 6) & 31, w & 63
    imm = w & 0xffff
    simm = imm - 0x10000 if imm & 0x8000 else imm
    if op in EXTRA:
        return f'{EXTRA[op]} {R[rt]}, {simm}({R[rs]})'
    if op in (0x16, 0x17):
        return f'{"blezl" if op == 0x16 else "bgtzl"} {R[rs]}, {va + 4 + simm * 4:#x}'
    if op == 0x11 and rs == 8:
        return f'{["bc1f", "bc1t", "bc1fl", "bc1tl"][rt & 3]} {va + 4 + simm * 4:#x}'
    if op == 0x11 and rs in (2, 6):
        return f'{"cfc1" if rs == 2 else "ctc1"} {R[rt]}, $fcr{rd}'
    if op == 0x12 and not (w >> 25 & 1) and rs == 8:
        return f'bc2{["f", "t", "fl", "tl"][rt & 3]} {va + 4 + simm * 4:#x}'
    if op == 0x12 and (w >> 25 & 1) and (w & 0x3f) in (0x38, 0x39):
        return f'VCALLMS{"R" if (w & 0x3f) == 0x39 else ""} {((w >> 6) & 0x7fff) * 8:#x}'
    if op == 0 and fn in (0x0c, 0x0d, 0x0f):
        return {0x0c: 'syscall', 0x0d: 'break', 0x0f: 'sync'}[fn]
    if op == 0 and fn in (0x14, 0x16, 0x17, 0x1a, 0x1b, 0x19, 0x11, 0x13, 0x2c, 0x2e, 0x01, 0x07, 0x3b, 0x3f, 0x28, 0x29):
        n = {0x14: 'dsllv', 0x16: 'dsrlv', 0x17: 'dsrav', 0x1a: 'div', 0x1b: 'divu', 0x19: 'multu', 0x11: 'mthi',
             0x13: 'mtlo', 0x2c: 'dadd', 0x2e: 'dsub', 0x07: 'srav', 0x3b: 'dsra', 0x3f: 'dsra32', 0x28: 'mfsa', 0x29: 'mtsa'}.get(fn, f'special?{fn:#x}')
        return f'{n} {R[rd]}, {R[rs]}, {R[rt]}' if fn not in (0x3b, 0x3f) else f'{n} {R[rd]}, {R[rt]}, {sa}'
    return eedis.dis(w, va)


if __name__ == '__main__':
    g = sys.argv[1]
    a, b = int(sys.argv[2], 16), int(sys.argv[3], 16)
    d = open(eedis.P[g], 'rb').read()
    sg = eedis.segs(d)
    for va in range(a, b, 4):
        o = eedis.off_of(sg, va)
        w = struct.unpack_from('<I', d, o)[0]
        print(f'{va:08x}: {w:08x}  {dis(w, va)}')
