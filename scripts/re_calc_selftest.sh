#!/usr/bin/env bash
# Fixture referee only; real BF1 evidence remains a separate acceptance obligation.
set -eu
export PYTHONDONTWRITEBYTECODE=1
python3 - "$(dirname "$0")" <<'PY'
import contextlib, io, os, subprocess, sys, time
from unittest.mock import patch
sys.path.insert(0, sys.argv[1])
import re_calc as r
start = time.perf_counter()
count = 0

def case(args, expected, error=False):
    global count
    out = io.StringIO()
    failed = False
    try:
        with contextlib.redirect_stdout(out):
            r.main(args.split())
    except (ValueError, OSError) as exc:
        failed = True
        out.write(str(exc))
    assert failed == error and expected in out.getvalue(), (args, out.getvalue(), error)
    count += 1

for args, expected in [
    ('int 0xffffffff', 's32=-1'), ('int -1', 'u32=4294967295'),
    ('int 0b1000000000000000', 'low u16=32768 s16=-32768'),
    ('int 65536', 'high u16=1 s16=1'), ('int 4294967296', 'u32=0'),
    ('int f32:1.0', '0x3f800000'), ('int bits:0x3f800000', '=1.0'),
    ('int f32:-0.0', '0x80000000'), ('int 0 0x3ff00000', '= 1.0 (low word first)'),
    ('int 0x7f800000', '=inf'), ('int 0x7fc00000', '=nan'),
    ('hilo 0x73 -0x2790 --op addiu', 'addiu picks 0x0072d870'),
    ('hilo 0x73 0xd870 --op ori', 'ori picks 0x0073d870'),
    ('hilo 0xffff -1 --op lw', 'lw picks 0xfffeffff; access width 4'),
    ('hilo 0 0xffff', '= 0xffffffff'),
    ('off 0x522ac8 -0x228 +4', '= 0x5228a4'),
    ('ptrchain 0x522ac8 +0 +0x228', 'load32(load32(0x522ac8) + 0x0) + 0x228'),
    ('va2off b2 0x60c1fc', '= 0x4ad1fc'),
    ('off2va b2 0x4ad1fc', '= 0x60c1fc'),
    ('va2off b1 0x1000 --reverse', '= 0x160000'),
    ('struct - a:0:u32 b:8:f32', 'gap: [0x4, 0x8), 4 bytes'),
    ('struct 16 a:0:u32 b:8:f32', 'trailing gap: 16 - 12 = 4'),
    ('struct - a:0:u8 b:4:ptr[2]', 'implied size = align_up(12, 4) = 12'),
    ('struct -', 'implied size = align_up(0, 1) = 0'),
]:
    case(args, expected)
for args, expected in [
    ('struct - a:0:u32 b:2:u16', 'overlap'),
    ('struct - a:1:u32', 'alignment'), ('struct 2 a:0:u32', 'declared size'),
    ('struct - a:0:xyz', 'invalid type'), ('struct - a:-1:u8', 'negative field'),
    ('struct - a:0:u8 a:1:u8', 'duplicate'), ('hilo 65536 0', 'lui must fit'),
    ('hilo 0 -32769', 'low half must fit'), ('hilo 0 0 --op xyz', 'unsupported'),
    ('va2off b1 0', 'below mapped'), ('hilo --elf b1 0x160001', 'word-aligned'),
]:
    case(args, expected, True)
# In-memory instruction bytes exercise the actual ELF seek/read path, not a real-ELF receipt.
import struct
for opcode, (name, width) in r.OPS.items():
    data = bytes(r.ELF_FILE_BASE) + struct.pack('<II', (15 << 26) | (2 << 16) | 0x73,
                                             (opcode << 26) | (2 << 21) | (3 << 16) | 0xd870)
    with patch('builtins.open', return_value=io.BytesIO(data)):
        case('hilo --elf b1 0x160000', f'{name} picks 0x00{ "73" if name == "ori" else "72" }d870; access width {width}')
for a, b, expected in [
    ((15 << 26) | (2 << 16), (9 << 26) | (3 << 21), 'using that base'),
    ((15 << 26), (9 << 26), 'nonzero'),
    ((15 << 26) | (2 << 16), (2 << 21), 'unsupported low'),
    (0, 0, 'expected lui'),
]:
    with patch('builtins.open', return_value=io.BytesIO(bytes(r.ELF_FILE_BASE) + struct.pack('<II', a, b))):
        case('hilo --elf b1 0x160000', expected, True)
with patch('builtins.open', return_value=io.BytesIO(b'')):
    case('hilo --elf b1 0x160000', 'truncated', True)
# Split pairs (the compiler schedules other work between lui and its low op).
LUI2, LUI3 = (15 << 26) | (2 << 16) | 0x67, (15 << 26) | (3 << 16) | 0x19
LW = (0x23 << 26) | (2 << 21) | (4 << 16) | 0xeed0
for mid, expected, error in [((LUI3,), 'lw picks 0x0066eed0; access width 4', False),
                             (((9 << 26) | (2 << 16),), 'redefine r2', True),  # addiu r2, r0, 0
                             (((2 << 11) | 0x21,), 'redefine r2', True),        # addu r2, r0, r0
                             (((4 << 26) | 1,), 'may branch', True)]:           # beq r0, r0
    words = (LUI2, *mid, LW)
    with patch('builtins.open', return_value=io.BytesIO(bytes(r.ELF_FILE_BASE) + struct.pack(f'<{len(words)}I', *words))):
        case(f'hilo --elf b1 0x160000 0x{0x160000 + 4 * (len(words) - 1):x}', expected, error)
case('hilo --elf b1 0x160000 0x160024', 'low VA 1..8 words', True)
case('hilo --elf b1 0x160000 0x160000', 'low VA 1..8 words', True)
# The real BF1 LCG load (FUN_001d9180, DAT_0066eed0), when the ELF is present.
if os.path.exists(os.path.join(r.ROOT, r.ELF['b1'])):
    case('hilo --elf b1 0x1d97d4 0x1d97dc', 'lw picks 0x0066eed0; access width 4')
    case('hilo --elf b1 0x1d97d4', 'using that base', True)
    real = 'real BF1 LCG pair PASS'
else:
    real = 'real BF1 LCG pair SKIPPED (no ' + r.ELF['b1'] + ')'
# Real entry point: signed arguments reach the parser; failures are concise exit 2.
for args, code, expected in [(['off', '0x100', '-4'], 0, '= 0xfc'),
                             (['int', 'garbage'], 2, 're_calc:')]:
    p = subprocess.run([sys.executable, os.path.join(sys.argv[1], 're_calc.py'), *args], capture_output=True, text=True)
    assert p.returncode == code and expected in p.stdout + p.stderr, p
    count += 1
print(f're_calc_selftest: PASS ({count} cases; {real}; {time.perf_counter()-start:.3f}s)')
PY
