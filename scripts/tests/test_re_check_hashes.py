"""RECHK/RCSRC fixtures; real corpus/disc acceptance remains separate."""
import contextlib
import io
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import re_check as rc

LOW, CONTROL, ABSENT = '0xa490eba1', '0x47c86b4a', '0xdeadbeef'


def pair(value, op=13, reg=8, dest=8):
    low = value & 0xffff
    high = ((value + (0x10000 if op == 9 and low >= 0x8000 else 0)) >> 16) & 0xffff
    return struct.pack('<II', (15 << 26) | (reg << 16) | high,
                       (op << 26) | (reg << 21) | (dest << 16) | low)


def elf(code):
    header = bytearray(52)
    header[:7] = b'\x7fELF\x01\x01\x01'
    struct.pack_into('<H', header, 18, 8)
    struct.pack_into('<I', header, 28, 52)
    struct.pack_into('<HH', header, 42, 32, 1)
    return header + struct.pack('<8I', 1, 84, 0x20f788, 0, len(code), len(code), 5, 4) + code


class HashSearch(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for name in ['a.c', 'b.c']:
            (self.root / name).write_text('/* ==== 0020f2e0 FUN_0020f2e0 ==== */\n')
        patch = mock.patch.multiple(rc, ROOT=str(self.root), CORP={'b1': 'a.c', 'b2': 'b.c'},
                                    ELF={'b1': 'a.elf', 'b2': 'b.elf'}, _cache={})
        patch.start()
        self.addCleanup(patch.stop)

    def test_each_text_form_with_controls_and_line_counts(self):
        forms = ['0xa490eba1', '0xffffffffa490eba1', '-0x5b6f145f', '2760960929', '-1534006367']
        self.assertEqual(set(rc._text_forms(LOW)), set(forms))
        for form in forms:
            with self.subTest(form=form):
                rc._cache.clear()
                (self.root / 'a.c').write_text('/* ==== 0020f2e0 FUN_0020f2e0 ==== */\n' +
                                              form + '; ' + form + ';\n0x47c86b4a;\n')
                rec = rc.search([LOW, CONTROL, ABSENT], quiet=True)
                self.assertEqual([r['hits'] for r in rec['corpora']], [1, 0, 1, 0, 0, 0])
                self.assertEqual(rec['corpora'][0]['forms'], rc._text_forms(LOW))

    def test_raw_word_offsets_and_no_corpus_delta(self):
        (self.root / 'rep.lvl').write_bytes(b'X' + struct.pack('<III', 0x6d4a8840, 0x129f08c1, 0x47c86b4a))
        (self.root / 'imp.lvl').write_bytes(struct.pack('<I', 0x47c86b4a))
        terms = ['0x6d4a8840', '0x129f08c1', CONTROL, ABSENT]
        baseline = rc.search(terms, quiet=True)
        rec = rc.search(terms, srcs=['rep.lvl', 'imp.lvl'], quiet=True)
        self.assertEqual(rec['corpora'], baseline['corpora'])
        rep, imp = rec['sources']
        self.assertEqual(rep['per_term'], dict(zip(terms, [1, 1, 1, 0])))
        self.assertEqual(imp['per_term'], dict(zip(terms, [0, 0, 1, 0])))
        self.assertEqual([m['offset'] for m in rep['matches']], [1, 5, 9])
        self.assertEqual(rep['needles'][terms[0]][1], {'form': 'le_u32', 'hex': '40884a6d'})

    def test_elf_builds_unsigned_and_signed_low_with_controls(self):
        for op in [13, 9]:
            with self.subTest(op=op):
                code = pair(int(LOW, 16), op) + pair(int(CONTROL, 16), op, dest=9)
                for name in ['a.elf', 'b.elf']:
                    (self.root / name).write_bytes(elf(code))
                out = io.StringIO()
                with contextlib.redirect_stdout(out):
                    rec = rc.search([LOW, CONTROL, ABSENT], elf=True)
                for source in rec['sources']:
                    self.assertEqual(source['per_term'], {LOW: 1, CONTROL: 1, ABSENT: 0})
                    self.assertEqual([m['va'] for m in source['matches']], [0x20f788, 0x20f790])
                    self.assertEqual([m['offset'] for m in source['matches']], [84, 92])
                self.assertIn('VA 0x20f788', out.getvalue())
                self.assertIn('other builds are not covered', out.getvalue())

    def test_bad_registers_nonexec_data_and_malformed_elf(self):
        for code in [pair(int(LOW, 16), reg=0), pair(int(LOW, 16), dest=0),
                     pair(int(LOW, 16), op=12)]:
            self.assertEqual(rc._code_builds(elf(code), {int(LOW, 16)}), [])
        data = bytearray(elf(pair(int(LOW, 16))))
        struct.pack_into('<I', data, 52 + 24, 4)
        self.assertEqual(rc._code_builds(data, {int(LOW, 16)}), [])
        for body in [b'', b'bad', elf(b'')[:60], elf(pair(int(LOW, 16)))[:-1]]:
            with self.subTest(length=len(body)), self.assertRaises(OSError):
                rc._code_builds(body, {int(LOW, 16)})

    def test_receipt_roundtrip_and_missing_source_fails(self):
        with mock.patch.dict('os.environ', {'OPENBF_LOOP_DIR': str(self.root / 'loop')}):
            rec = rc.search([LOW, CONTROL, ABSENT], quiet=True)
            rid = rc.write_receipt(rec, 'RECHK')
            import json
            written = json.loads((self.root / 'loop/receipts' / f'RECHK_{rid}.json').read_text())
            self.assertEqual(written['version'], 2)
            self.assertEqual(written['corpora'][0]['forms'], rc._text_forms(LOW))
        with self.assertRaises(OSError):
            rc.search([LOW], srcs=['missing'], quiet=True)

    def test_split_builds_within_window_and_register_rewrite(self):
        value = int(CONTROL, 16)
        hi = struct.pack('<I', (15 << 26) | (5 << 16) | (value >> 16))
        ori = struct.pack('<I', (13 << 26) | (5 << 21) | (5 << 16) | (value & 0xffff))
        nop, jal = struct.pack('<I', 0), struct.pack('<I', (3 << 26) | 0x78d9e)
        relui = struct.pack('<I', (15 << 26) | (5 << 16) | 0x3403)
        jump = struct.pack('<I', (2 << 26) | 0x100)
        split = rc._code_builds(elf(hi + nop + nop + jal + ori), {value})
        self.assertEqual([(m['offset'], m['consumer_va']) for m in split], [(84, 0x20f788 + 16)])
        self.assertEqual(rc._code_builds(elf(hi + relui + ori), {value}), [])
        self.assertEqual(rc._code_builds(elf(hi + jump + nop + ori), {value}), [])
        self.assertEqual(len(rc._code_builds(elf(hi + jump + ori), {value})), 1)
        self.assertEqual(rc._code_builds(elf(hi + nop * rc.BUILD_WINDOW + ori), {value}), [])
        self.assertEqual(len(rc._code_builds(elf(hi + nop * (rc.BUILD_WINDOW - 1) + ori), {value})), 1)

    def test_decimal_forms_need_a_leading_boundary(self):
        (self.root / 'a.c').write_text('/* ==== 0020f2e0 FUN_0020f2e0 ==== */\nDAT_00201480 = 0;\nx = 1480;\n'
                                      'y = -0x5b6f145f;\n')
        rec = rc.search(['0x5c8', LOW], quiet=True)
        self.assertEqual([r['hits'] for r in rec['corpora']], [1, 0, 1, 0])

    def test_unaligned_raw_word_is_flagged(self):
        (self.root / 'end.lvl').write_bytes(b'XY' + struct.pack('<I', 0x70bb9aa6) + struct.pack('<I', 0x70bb9aa6))
        rec = rc.search(['0x70bb9aa6'], srcs=['end.lvl'], quiet=True)
        self.assertEqual([m['aligned'] for m in rec['sources'][0]['matches']], [False, False])

    def test_delay_slot_lui_follows_the_branch_target(self):
        value = int(CONTROL, 16)
        bne = struct.pack('<I', (5 << 26) | (2 << 21) | 3)  # bne v0, zero, +3 -> target = delay slot + 3 words
        hi = struct.pack('<I', (15 << 26) | (2 << 16) | (value >> 16))
        ori = struct.pack('<I', (13 << 26) | (2 << 21) | (2 << 16) | (value & 0xffff))
        lw = struct.pack('<I', (35 << 26) | (17 << 21) | (2 << 16) | 0x13dc)
        nop = struct.pack('<I', 0)
        code = bne + hi + nop + lw + ori  # fall-through rewrites v0; the target (ori) is the build
        builds = rc._code_builds(elf(code), {value})
        self.assertEqual([(b['offset'], b['via']) for b in builds], [(88, 'branch target')])
        jal = struct.pack('<I', (3 << 26) | ((0x20f788 + 16) >> 2))
        self.assertEqual(rc._code_builds(elf(jal + hi + nop + lw + ori), {value}), [])


if __name__ == '__main__':
    unittest.main()
