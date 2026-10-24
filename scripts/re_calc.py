#!/usr/bin/env python3
"""Deterministic RE arithmetic; no RAM reads or writes. Examples:
  re_calc.py int 0x3f800000 f32:1.0
  re_calc.py off 0x522ac8 +0x228 -4
  re_calc.py ptrchain 0x522ac8 +0 +0x228
  re_calc.py hilo 0x73 -0x2790 [--op addiu|ori|lw|...]
  re_calc.py hilo --elf b1 0xVA [0xLO_VA]   # lui VA (+ low op VA, <= 8 words on), not the data VA
  re_calc.py va2off b2 0x60c1fc [--reverse]
  re_calc.py struct 16 x:0:u32 y:8:f32   # size '-' infers; types u/s8/16/32/64, f32/64, ptr, TYPE[N]
Two int words give a little-endian f64 (low word first). off uses unbounded
integers; hilo wraps to u32. Struct gaps are reported; overlap/alignment fail.
"""
import os
import re
import struct
import sys
sys.dont_write_bytecode = True
from re_check import ELF, ROOT, ELF_VA_BASE, ELF_FILE_BASE

MASK = 0xffffffff
OPS = {0x09: ('addiu', 0), 0x0d: ('ori', 0), 0x20: ('lb', 1), 0x21: ('lh', 2),
       0x23: ('lw', 4), 0x28: ('sb', 1), 0x29: ('sh', 2), 0x2b: ('sw', 4),
       0x37: ('ld', 8), 0x3f: ('sd', 8), 0x31: ('lwc1', 4), 0x39: ('swc1', 4),
       0x1e: ('lq', 16), 0x1f: ('sq', 16)}
WRITES_RT = {*range(0x08, 0x10), 0x18, 0x19, 0x1a, 0x1b, 0x1e, *range(0x20, 0x28), 0x37}
NO_GPR_WRITE = {*range(0x28, 0x30), 0x1f, 0x31, 0x33, 0x36, 0x39, 0x3e, 0x3f}


def clobbers(m, reg):
    """May word m (between lui and the low op) branch or write GPR reg? Unknown -> True."""
    op, rs, rt, rd = m >> 26, (m >> 21) & 31, (m >> 16) & 31, (m >> 11) & 31
    if op in (1, 2, 3, 4, 5, 6, 7, 0x14, 0x15, 0x16, 0x17):
        return True
    if op == 0:
        return (m & 63) in (8, 9) or rd == reg
    if op == 0x1c:
        return rd == reg
    if op in (0x10, 0x11, 0x12, 0x13):
        return rs == 8 or (rs in (0, 1, 2) and rt == reg)
    return rt == reg if op in WRITES_RT else op not in NO_GPR_WRITE


def number(s):
    if s.startswith('f32:'):
        return struct.unpack('<I', struct.pack('<f', float(s[4:])))[0]
    if s.startswith('bits:'):
        s = s[5:]
    return int(s, 0) if s.lstrip('+-').lower().startswith(('0x', '0b', '0o')) else int(s, 10)


def signed(n, bits):
    n &= (1 << bits) - 1
    return n - (1 << bits) if n & (1 << (bits - 1)) else n


def pair(hi, lo, op=None):
    if not 0 <= hi <= 65535 or not -32768 <= lo <= 65535:
        raise ValueError('lui must fit u16; low half must fit s16 or u16')
    top, low = hi << 16, lo & 65535
    add, ori = (top + signed(low, 16)) & MASK, top | low
    print(f'addiu/address: ((0x{hi:04x} << 16) + s16(0x{low:04x}) [{signed(low, 16)}]) & 0xffffffff = 0x{add:08x}')
    print(f'ori: (0x{hi:04x} << 16) | u16(0x{low:04x}) = 0x{ori:08x}')
    if op:
        widths = dict(OPS.values())
        if op not in widths:
            raise ValueError('unsupported low opcode: ' + op)
        print(f'{op} picks 0x{(ori if op == "ori" else add):08x}; access width {widths[op]} bytes')
    else:
        print('opcode unspecified: addiu and memory offsets pick signed add; ori picks unsigned OR')


def fileoff(va):
    return va - ELF_VA_BASE + ELF_FILE_BASE


def decode(c, va, lo_va=None):
    lo_va = va + 4 if lo_va is None else lo_va
    if c not in ELF or va % 4 or lo_va % 4 or fileoff(va) < ELF_FILE_BASE or not va < lo_va <= va + 32:
        raise ValueError('expected b1|b2, a mapped word-aligned lui VA and a low VA 1..8 words after it')
    path = os.path.join(ROOT, ELF[c])
    n = (lo_va - va) // 4 + 1
    with open(path, 'rb') as f:
        f.seek(fileoff(va))
        data = f.read(4 * n)
    if len(data) != 4 * n:
        raise ValueError('ELF pair truncated/out of range')
    words = struct.unpack(f'<{n}I', data)
    a, b = words[0], words[-1]
    rt, rs = (a >> 16) & 31, (b >> 21) & 31
    if a >> 26 != 15 or (a >> 21) & 31 or not rt or rs != rt:
        raise ValueError('expected lui to a nonzero register and low instruction using that base')
    if b >> 26 not in OPS:
        raise ValueError('unsupported low instruction')
    for i, m in enumerate(words[1:-1], 1):
        if clobbers(m, rt):
            raise ValueError(f'0x{va + 4 * i:x}: 0x{m:08x} may branch or redefine r{rt}; pair not proven')
    op, width = OPS[b >> 26]
    print(f'{path}: off = 0x{va:x} - 0x{ELF_VA_BASE:x} + 0x{ELF_FILE_BASE:x} = 0x{fileoff(va):x}')
    print(f'0x{va:x}: 0x{a:08x} lui r{rt}; 0x{lo_va:x}: 0x{b:08x} {op} r{(b >> 16) & 31}, base r{rs}')
    for i, m in enumerate(words[1:-1], 1):
        print(f'  between: 0x{va + 4 * i:x}: 0x{m:08x} (no branch, does not write r{rt})')
    pair(a & 65535, b & 65535, op)


def layout(args):
    size = None if args[0] == '-' else number(args[0])
    fields, names = [], set()
    for arg in args[1:]:
        name, off, typ = arg.split(':')
        m = re.fullmatch(r'(ptr|[us](?:8|16|32|64)|f(?:32|64))(?:\[([1-9][0-9]*)\])?', typ)
        if not m or not name or name in names:
            raise ValueError('invalid type or duplicate/empty field name: ' + arg)
        names.add(name)
        width = 4 if m[1] == 'ptr' else int(m[1][1:]) // 8
        start, length = number(off), width * int(m[2] or 1)
        if start < 0:
            raise ValueError('negative field offset')
        fields.append((start, start + length, width, name))
    end, align, errors = 0, 1, []
    for start, stop, width, name in sorted(fields):
        if start < end:
            errors.append(f'overlap at {name}: 0x{start:x} < previous end 0x{end:x}')
        if start > end:
            print(f'gap: [0x{end:x}, 0x{start:x}), {start-end} bytes')
        if start % width:
            errors.append(f'alignment: {name} offset {start} % {width} != 0')
        print(f'{name}: 0x{start:x} + {stop-start} bytes = 0x{stop:x} (exclusive end)')
        end, align = max(end, stop), max(align, width)
    implied = (end + align - 1) // align * align
    print(f'implied size = align_up({end}, {align}) = {implied} (0x{implied:x})')
    if size is not None:
        if size < implied or size % align:
            errors.append(f'declared size {size} must be >= {implied} and aligned to {align}')
        if size > end:
            print(f'trailing gap: {size} - {end} = {size-end} bytes')
    if errors:
        raise ValueError('; '.join(errors))


def main(args):
    if not args or args[0] in ('-h', '--help'):
        print(__doc__)
        return
    cmd, a = args[0], args[1:]
    if cmd == 'int' and a:
        words = [number(x) & MASK for x in a]
        for raw, u in zip(a, words):
            print(f'{raw} & 0xffffffff = 0x{u:08x}; u32={u}; s32={signed(u,32)}; f32=bits_to_f32(0x{u:08x})={struct.unpack("<f", struct.pack("<I", u))[0]!r}')
            print(f'  low u16={u & 65535} s16={signed(u,16)}; high u16={u >> 16} s16={signed(u >> 16,16)} (n & 0xffff; n >> 16)')
        if len(words) == 2:
            print(f'f64 = bits_to_f64((0x{words[1]:08x} << 32) | 0x{words[0]:08x}) = {struct.unpack("<d", struct.pack("<II", *words))[0]!r} (low word first)')
    elif cmd in ('off', 'ptrchain') and a:
        nums = [number(x) for x in a]
        expr = hex(nums[0])
        for n in nums[1:]:
            expr = (f'load32({expr})' if cmd == 'ptrchain' else expr)
            expr += f' {"+" if n >= 0 else "-"} 0x{abs(n):x}'
        print(expr + (f' = {hex(sum(nums))} ({sum(nums)})' if cmd == 'off' else ' (expression only; no memory read)'))
    elif cmd == 'hilo':
        if len(a) in (3, 4) and a[0] == '--elf':
            decode(a[1], number(a[2]), number(a[3]) if len(a) == 4 else None)
        elif len(a) in (2, 4) and (len(a) == 2 or a[2] == '--op'):
            pair(number(a[0]), number(a[1]), a[3] if len(a) == 4 else None)
        else:
            raise ValueError('hilo HI LO [--op OPCODE] or hilo --elf b1|b2 VA [LO_VA]')
    elif cmd in ('va2off', 'off2va') and len(a) in (2, 3) and a[0] in ELF:
        if len(a) == 3 and a[2] != '--reverse':
            raise ValueError('expected --reverse')
        n = number(a[1])
        reverse = cmd == 'off2va' or len(a) == 3
        x, y = (ELF_FILE_BASE, ELF_VA_BASE) if reverse else (ELF_VA_BASE, ELF_FILE_BASE)
        if n < x:
            raise ValueError('address/offset below mapped segment')
        print(f'{a[0]} {cmd}: 0x{n:x} - 0x{x:x} + 0x{y:x} = 0x{n-x+y:x}')
    elif cmd == 'struct' and a:
        layout(a)
    else:
        raise ValueError('invalid command/arguments; use --help')


if __name__ == '__main__':
    try:
        main(sys.argv[1:])
    except (ValueError, OSError, OverflowError, struct.error) as exc:
        print(f're_calc: {exc}', file=sys.stderr)
        sys.exit(2)
