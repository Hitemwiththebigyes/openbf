#!/usr/bin/env python3
"""extract_lua.py -- extract mission/shell Lua scripts from SWBF PS2 ucfb LVL files.

Container format (both games):
    [4-byte tag][u32 LE payload size][payload][pad to 4]
Top-level chunk is 'ucfb'. Script carriers:

  BF1 MISSION.LVL:  ucfb -> lvl_ ([u32 fnv1a(name)][u32 inner_size] prefix)
                         -> scr_ -> NAME (cstr), INFO (u8), BODY (Lua 4.0 dump)
  BF2 MISSION.LVL:  ucfb -> scr_ (top level, plain NAME) and
                    ucfb -> lvl_ ([u32 hash][u32 inner_size]) -> scr_ | mcfg
                    mcfg -> NAME (4-byte hash), then SCOP/DATA config chunks
                    scr_ BODY is a Lua 5.0 dump.

Bytecode detection: BODY starting with 1B 4C 75 61 ('\\x1bLua') + version byte
(0x40 = Lua 4.0, 0x50 = Lua 5.0). Anything else is treated as source text.

Usage:
    extract_lua.py FILE [FILE ...] --out DIR [--census-only] [--strings]
                   [--names FILE ...]

  --out DIR       dump scripts there: <name>.luac / <name>.lua for scr_,
                  mcfg_<hash8>.bin for mcfg chunks
  --census-only   print the census but write nothing
  --strings       also print each script's embedded string constants
  --names FILE    extra candidate-name wordlists (one per line) for matching
                  4-byte hashes (mcfg NAMEs, lvl_ prefixes) via FNV-1a-32
"""

import argparse
import os
import string
import struct
import sys

PRINTABLE = set(string.ascii_letters + string.digits + "_ .-")
TEXTCHARS = set(range(32, 127)) | {9, 10, 13}

FNV_BASIS = 0x811C9DC5
FNV_PRIME = 0x1000193


def fnv1a(name: str) -> int:
    """Zero-engine 32-bit FNV-1a over the lowercased name."""
    h = FNV_BASIS
    for c in name.lower().encode("ascii", "replace"):
        h ^= c
        h = (h * FNV_PRIME) & 0xFFFFFFFF
    return h


def tag_ok(raw: bytes) -> bool:
    return len(raw) == 4 and all(chr(b) in PRINTABLE for b in raw)


def iter_chunks(buf, off, end):
    """Yield (tag, body_off, size) for the flat chunk sequence in buf[off:end)."""
    pos = off
    while pos + 8 <= end:
        tag = buf[pos:pos + 4]
        (size,) = struct.unpack_from("<I", buf, pos + 4)
        body = pos + 8
        if not tag_ok(tag) or body + size > end:
            return
        yield tag.decode("ascii"), body, size
        pos = body + size + (-size % 4)


def read_cstr(buf, off, size):
    return buf[off:off + size].split(b"\0", 1)[0].decode("ascii", "replace")


# ---------------------------------------------------------------- Lua headers

def parse_lua_header(body: bytes):
    """Parse a Lua 4.0 / 5.0 dump header. Returns a dict or None if not bytecode."""
    if len(body) < 14 or body[:4] != b"\x1bLua":
        return None
    ver = body[4]
    if ver == 0x40:  # Lua 4.0: endian, int, size_t, Instruction, SIZE_INSTRUCTION,
        #             SIZE_B, SIZE_OP(?), sizeof(Number), test number
        (endian, si, ss, sinstr, bits_instr, bits_b, bits_op,
         snum) = struct.unpack_from("<8B", body, 5)
        fmt = "<f" if snum == 4 else "<d"
        (test,) = struct.unpack_from(fmt, body, 13)
        return dict(version="4.0", endian=endian, sizeof_int=si,
                    sizeof_size_t=ss, sizeof_instr=sinstr,
                    sizeof_number=snum, test_number=test)
    if ver == 0x50:  # Lua 5.0: endian, int, size_t, Instruction,
        #             SIZE_OP, SIZE_A, SIZE_B, SIZE_C, sizeof(Number), test
        (endian, si, ss, sinstr, sop, sa, sb, sc,
         snum) = struct.unpack_from("<9B", body, 5)
        fmt = "<f" if snum == 4 else "<d"
        (test,) = struct.unpack_from(fmt, body, 14)
        return dict(version="5.0", endian=endian, sizeof_int=si,
                    sizeof_size_t=ss, sizeof_instr=sinstr,
                    sizeof_number=snum, test_number=test)
    return dict(version=f"0x{ver:02X}?", endian=None, sizeof_int=None,
                sizeof_size_t=None, sizeof_instr=None, sizeof_number=None,
                test_number=None)


def extract_strings(body: bytes, min_len=2, max_len=512):
    """Walk the dump for [u32 len][bytes...\\0] string records (len includes the
    NUL, as both Lua 4.0 and 5.0 dump formats store it). Simple linear scan --
    no opcode decoding; good enough to recover constants/identifiers."""
    out = []
    i = 0
    n = len(body)
    while i + 4 < n:
        (ln,) = struct.unpack_from("<I", body, i)
        if min_len <= ln <= max_len and i + 4 + ln <= n and body[i + 4 + ln - 1] == 0:
            raw = body[i + 4:i + 4 + ln - 1]
            if raw and all(b in TEXTCHARS for b in raw):
                out.append(raw.decode("ascii"))
                i += 4 + ln
                continue
        i += 1
    return out


# ---------------------------------------------------------------- tree walk

def collect_scripts(buf, off, end, found, source_tag=""):
    """Recursively collect scr_ and mcfg chunks. lvl_ payloads carry an
    8-byte [hash][inner_size] prefix before their child chunk sequence."""
    for tag, body, size in iter_chunks(buf, off, end):
        if tag == "lvl_" and size >= 8:
            (h, inner) = struct.unpack_from("<II", buf, body)
            if inner <= size - 8:
                collect_scripts(buf, body + 8, body + 8 + inner, found,
                                source_tag=f"lvl_[{h:08x}]")
            continue
        if tag == "scr_":
            name, info, sbody = None, None, None
            for ctag, cbody, csize in iter_chunks(buf, body, body + size):
                if ctag == "NAME":
                    name = read_cstr(buf, cbody, csize)
                elif ctag == "INFO":
                    info = buf[cbody] if csize else None
                elif ctag == "BODY":
                    sbody = bytes(buf[cbody:cbody + csize])
            found.append(dict(kind="scr_", name=name, info=info,
                              body=sbody, wrapper=source_tag))
        elif tag == "mcfg":
            name_hash, payload_off = None, body
            for ctag, cbody, csize in iter_chunks(buf, body, body + size):
                if ctag == "NAME" and csize == 4:
                    (name_hash,) = struct.unpack_from("<I", buf, cbody)
                    break
            found.append(dict(kind="mcfg", name=None, hash=name_hash,
                              body=bytes(buf[body:body + size]),
                              wrapper=source_tag))
        elif tag in ("ucfb",):
            collect_scripts(buf, body, body + size, found)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("files", nargs="+")
    ap.add_argument("--out", default=None,
                    help="output dir (default: extracted/<game>/lua/ guessed "
                         "from the input path)")
    ap.add_argument("--census-only", action="store_true")
    ap.add_argument("--strings", action="store_true",
                    help="print embedded string constants for each script")
    ap.add_argument("--names", nargs="*", default=[],
                    help="wordlist files of candidate names for hash matching")
    args = ap.parse_args()

    # candidate names for hash back-matching
    candidates = {}
    for wl in args.names:
        with open(wl, encoding="utf-8") as f:
            for line in f:
                w = line.strip()
                if w:
                    candidates[fnv1a(w)] = w

    grand = []
    for path in args.files:
        with open(path, "rb") as f:
            buf = f.read()
        if buf[:4] != b"ucfb":
            print(f"{path}: no ucfb magic, skipping", file=sys.stderr)
            continue

        found = []
        collect_scripts(buf, 0, len(buf), found)

        # every scr_ name is itself a hash candidate
        for s in found:
            if s["kind"] == "scr_" and s["name"]:
                candidates.setdefault(fnv1a(s["name"]), s["name"])

        out_dir = args.out
        if out_dir is None:
            lower = path.lower()
            game = "bf1" if "/bf1/" in lower else "bf2" if "/bf2/" in lower else "out"
            out_dir = os.path.join("extracted", game, "lua")

        print(f"\n=== {path}: {len(found)} script chunk(s) ===")
        vers = {}
        for s in found:
            body = s["body"] or b""
            hdr = parse_lua_header(body)
            if s["kind"] == "scr_":
                base = s["name"] or "unnamed"
                kind = (f"Lua {hdr['version']} bytecode" if hdr else "source text")
                if hdr:
                    vers[hdr["version"]] = vers.get(hdr["version"], 0) + 1
                    ext = ".luac"
                else:
                    ext = ".lua"
                print(f"  scr_ {base:<24} {len(body):6d} B  {kind}"
                      + (f"  (Number={hdr['sizeof_number']}B,"
                         f" test={hdr['test_number']:.6g})" if hdr and
                         hdr["sizeof_number"] else ""))
            else:
                h = s["hash"]
                match = candidates.get(h) if h is not None else None
                base = f"mcfg_{h:08x}" if h is not None else "mcfg_nohash"
                ext = ".bin"
                print(f"  mcfg {base:<24} {len(body):6d} B  config"
                      + (f"  name={match}" if match else "  name=?"))
            if args.strings and s["kind"] == "scr_":
                for st in extract_strings(body):
                    print(f"        | {st}")
            if not args.census_only:
                os.makedirs(out_dir, exist_ok=True)
                fn = os.path.join(out_dir, base + ext)
                with open(fn, "wb") as f:
                    f.write(body)
            s["base"] = base
        grand.extend(found)

        n_scr = sum(1 for s in found if s["kind"] == "scr_")
        n_cfg = len(found) - n_scr
        print(f"  -- {n_scr} scr_, {n_cfg} mcfg; Lua versions: {vers or 'n/a'}")

    # hash-match report over all mcfg chunks seen
    hashes = [s["hash"] for s in grand if s["kind"] == "mcfg" and s["hash"] is not None]
    if hashes:
        matched = sum(1 for h in hashes if h in candidates)
        print(f"\nmcfg NAME hash matching: {matched}/{len(hashes)} resolved "
              f"({len(candidates)} candidate names)")
        for h in sorted(set(hashes)):
            print(f"  {h:08x} -> {candidates.get(h, '?')}")


if __name__ == "__main__":
    main()
