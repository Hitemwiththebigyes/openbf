"""eedis.py <bf1|bf2> <va_start> <va_end> -- a small R5900 disassembler for the SPEC RE (lane 1, 2026-09-24): the MIPS ops
that matter to FUN_00589ac0-class code (loads/stores incl. LQC2/SQC2, lui/ori/addiu, jal/jr/branches, FPU lwc1/swc1/mul.s)
plus COP2 macro-mode VU0 instructions decoded with vu1dis.py's upper-op decoder, so the broadcast field (.x/.y/.z/.w) the
Ghidra listing drops (`_vmulabc`) is printed. BF2 file off = va - 0x160000 + 0x1000; BF1 per its PT_LOAD (elfw.py ph)."""
import os, struct, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import vu1dis

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..') + os.sep  # the repo root
P = {'bf2': ROOT + 'extracted/bf2/SWBFRNT2.ELF', 'bf1': ROOT + 'extracted/bf1/SWBFRONT.ELF'}
R = ['zero', 'at', 'v0', 'v1', 'a0', 'a1', 'a2', 'a3', 't0', 't1', 't2', 't3', 't4', 't5', 't6', 't7', 's0', 's1', 's2',
     's3', 's4', 's5', 's6', 's7', 't8', 't9', 'k0', 'k1', 'gp', 'sp', 'fp', 'ra']


def segs(d):
    phoff = struct.unpack_from('<I', d, 0x1c)[0]
    phes, phn = struct.unpack_from('<HH', d, 0x2a)
    out = []
    for i in range(phn):
        t, off, va, pa, fsz, msz, fl, al = struct.unpack_from('<IIIIIIII', d, phoff + i * phes)
        if t == 1:
            out.append((va, off, fsz))
    return out


def off_of(sg, va):
    for sva, off, fsz in sg:
        if sva <= va < sva + fsz:
            return off + va - sva
    raise ValueError(hex(va))


def dis(w, va):
    op = w >> 26
    rs, rt, rd, sa, fn = (w >> 21) & 31, (w >> 16) & 31, (w >> 11) & 31, (w >> 6) & 31, w & 63
    imm = w & 0xffff
    simm = imm - 0x10000 if imm & 0x8000 else imm
    if w == 0:
        return 'nop'
    if op == 0:
        n = {0x00: 'sll', 0x02: 'srl', 0x03: 'sra', 0x08: 'jr', 0x09: 'jalr', 0x21: 'addu', 0x23: 'subu', 0x24: 'and',
             0x25: 'or', 0x2d: 'daddu', 0x2b: 'sltu', 0x2a: 'slt', 0x0a: 'movz', 0x0b: 'movn', 0x38: 'dsll',
             0x3c: 'dsll32', 0x3e: 'dsrl32', 0x3a: 'dsrl', 0x27: 'nor', 0x26: 'xor', 0x18: 'mult', 0x10: 'mfhi',
             0x12: 'mflo', 0x04: 'sllv', 0x06: 'srlv', 0x2f: 'dsubu'}.get(fn, f'special?{fn:#x}')
        if n in ('sll', 'srl', 'sra', 'dsll', 'dsll32', 'dsrl32', 'dsrl'):
            return f'{n} {R[rd]}, {R[rt]}, {sa}'
        if n == 'jr':
            return f'jr {R[rs]}'
        if n == 'jalr':
            return f'jalr {R[rd]}, {R[rs]}'
        return f'{n} {R[rd]}, {R[rs]}, {R[rt]}'
    if op in (2, 3):
        t = (va & 0xf0000000) | ((w & 0x3ffffff) << 2)
        return f'{"j" if op == 2 else "jal"} {t:#x}'
    if op in (4, 5, 0x14, 0x15):
        n = {4: 'beq', 5: 'bne', 0x14: 'beql', 0x15: 'bnel'}[op]
        return f'{n} {R[rs]}, {R[rt]}, {va + 4 + simm * 4:#x}'
    if op in (6, 7):
        return f'{"blez" if op == 6 else "bgtz"} {R[rs]}, {va + 4 + simm * 4:#x}'
    if op == 1:
        n = {0: 'bltz', 1: 'bgez', 2: 'bltzl', 3: 'bgezl'}.get(rt, f'regimm?{rt}')
        return f'{n} {R[rs]}, {va + 4 + simm * 4:#x}'
    ii = {8: 'addi', 9: 'addiu', 0xa: 'slti', 0xb: 'sltiu', 0xc: 'andi', 0xd: 'ori', 0xe: 'xori', 0x19: 'daddiu'}
    if op in ii:
        v = imm if op in (0xc, 0xd, 0xe) else simm
        return f'{ii[op]} {R[rt]}, {R[rs]}, {v:#x}' if op in (0xc, 0xd, 0xe) else f'{ii[op]} {R[rt]}, {R[rs]}, {v}'
    if op == 0xf:
        return f'lui {R[rt]}, {imm:#x}'
    mem = {0x20: 'lb', 0x21: 'lh', 0x23: 'lw', 0x24: 'lbu', 0x25: 'lhu', 0x28: 'sb', 0x29: 'sh', 0x2b: 'sw', 0x37: 'ld',
           0x3f: 'sd', 0x1e: 'lq', 0x1f: 'sq', 0x31: 'lwc1', 0x39: 'swc1', 0x36: 'lqc2', 0x3e: 'sqc2'}
    if op in mem:
        n = mem[op]
        reg = f'$f{rt}' if n in ('lwc1', 'swc1') else (f'vf{rt}' if n in ('lqc2', 'sqc2') else R[rt])
        return f'{n} {reg}, {simm}({R[rs]})'
    if op == 0x11:  # COP1
        fmt = rs
        if fmt == 0x10:  # S
            ft, fs_, fd = rt, rd, sa
            n = {0: 'add.s', 1: 'sub.s', 2: 'mul.s', 3: 'div.s', 6: 'mov.s', 7: 'neg.s', 0x24: 'cvt.w.s',
                 0x18: 'adda.s', 0x1a: 'mula.s', 0x1c: 'madd.s', 0x1d: 'msub.s', 0x04: 'sqrt.s', 0x16: 'rsqrt.s',
                 0x28: 'max.s', 0x29: 'min.s', 0x32: 'c.eq.s', 0x34: 'c.lt.s', 0x36: 'c.le.s'}.get(fn, f'cop1s?{fn:#x}')
            return f'{n} $f{fd}, $f{fs_}, $f{ft}'
        if fmt == 0x14:
            return f'cvt.s.w $f{sa}, $f{rd}'
        if fmt == 0:
            return f'mfc1 {R[rt]}, $f{rd}'
        if fmt == 4:
            return f'mtc1 {R[rt]}, $f{rd}'
        return f'cop1?{w:08x}'
    if op == 0x12:  # COP2
        if w >> 25 & 1:
            f11 = w & 0x7ff
            if (f11 & 0x3c) == 0x3c and f11 not in vu1dis.UP11:
                # macro-mode lower-class op (VDIV/VSQRT/VRSQRT/VWAITQ/VMOVE/VMTIR...): same bits 24-0 as a VU lower special
                return 'V' + vu1dis.lower(0, (0x40 << 25) | (w & 0x1ffffff))
            return 'V' + vu1dis.upper(w & 0x1ffffff)
        sub = rs
        n = {1: 'qmfc2', 2: 'cfc2', 5: 'qmtc2', 6: 'ctc2'}.get(sub, f'cop2?{sub}')
        return f'{n} {R[rt]}, vf{rd}' if n.startswith('q') else f'{n} {R[rt]}, vi{rd}'
    if op == 0x1c:  # MMI
        return f'mmi {w:08x}'
    return f'op{op:#x}? {w:08x}'


if __name__ == '__main__':
    g = sys.argv[1]
    a, b = int(sys.argv[2], 16), int(sys.argv[3], 16)
    d = open(P[g], 'rb').read()
    sg = segs(d)
    for va in range(a, b, 4):
        o = off_of(sg, va)
        w = struct.unpack_from('<I', d, o)[0]
        print(f'{va:08x} (file {o:#x}): {w:08x}  {dis(w, va)}')
