#!/usr/bin/env python3
"""Scan a PS2 ELF .data section for {char* name, void* func} pair tables
(luaL_reg-style script binding arrays). Emits: <func_addr> <name> lines."""
import struct, sys, re

IDENT = re.compile(rb'^[A-Za-z_][A-Za-z0-9_.]*$')

def sections(f):
    f.seek(0)
    eh = f.read(52)
    assert eh[:4] == b'\x7fELF'
    shoff, = struct.unpack('<I', eh[32:36])
    shentsize, shnum, shstrndx = struct.unpack('<HHH', eh[46:52])
    f.seek(shoff)
    raw = [f.read(shentsize) for _ in range(shnum)]
    hdrs = [struct.unpack('<10I', s[:40]) for s in raw]
    # name,type,flags,addr,offset,size,link,info,align,entsize
    strtab_off, strtab_size = hdrs[shstrndx][4], hdrs[shstrndx][5]
    f.seek(strtab_off)
    st = f.read(strtab_size)
    out = {}
    for h in hdrs:
        name = st[h[0]:st.index(b'\0', h[0])].decode()
        out[name] = {'addr': h[3], 'off': h[4], 'size': h[5], 'type': h[1]}
    return out

def load(f, sec):
    f.seek(sec['off'])
    return f.read(sec['size'])

def main(path):
    f = open(path, 'rb')
    secs = sections(f)
    text, data, rodata = secs['.text'], secs['.data'], secs['.rodata']
    tlo, thi = text['addr'], text['addr'] + text['size']
    blobs = {}          # vaddr-indexed readable blobs for string lookup
    for s in (data, rodata):
        blobs[s['addr']] = (load(f, s), s['size'])

    def cstr(va):
        for base, (blob, size) in blobs.items():
            if base <= va < base + size:
                end = blob.find(b'\0', va - base)
                if end < 0: return None
                return blob[va - base:end]
        return None

    dblob = blobs[data['addr']][0]
    base = data['addr']
    pairs = []          # (data_vaddr, func_vaddr, name)
    n = len(dblob) // 4
    words = struct.unpack(f'<{n}I', dblob[:n*4])
    for i in range(n - 1):
        namep, funcp = words[i], words[i+1]
        if not (tlo <= funcp < thi) or funcp & 3: continue
        s = cstr(namep)
        if not s or not (2 <= len(s) <= 64) or not IDENT.match(s): continue
        pairs.append((base + i*4, funcp, s.decode()))

    # keep only pairs that belong to runs (stride 8) of >= 4 entries: real tables
    keep, run = [], []
    for p in pairs:
        if run and p[0] - run[-1][0] == 8:
            run.append(p)
        else:
            if len(run) >= 4: keep += run
            run = [p]
    if len(run) >= 4: keep += run

    seen = set()
    for va, func, name in keep:
        if func in seen: continue
        seen.add(func)
        print(f'{func:08x} {name}')
    print(f'# tables: {len(keep)} entries, {len(seen)} unique functions', file=sys.stderr)

if __name__ == '__main__':
    main(sys.argv[1])
