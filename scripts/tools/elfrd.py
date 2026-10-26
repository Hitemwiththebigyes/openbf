#!/usr/bin/env python3
"""elfrd.py <elf> <va hex> [n=16] - print n 32-bit words of a PS2 ELF from virtual address va (hex), each as
hex and as a float, mapping VA to file offset through the ELF's PT_LOAD headers. Also importable: Elf(path).u32/f32/rd."""
import struct,sys
class Elf:
    def __init__(s,path):
        s.d=open(path,'rb').read()
        e=s.d
        assert e[:4]==b'\x7fELF'
        phoff=struct.unpack_from('<I',e,0x1c)[0]; phentsize,phnum=struct.unpack_from('<HH',e,0x2a)
        s.segs=[]
        for i in range(phnum):
            t,off,va,pa,fsz,msz,fl,al=struct.unpack_from('<IIIIIIII',e,phoff+i*phentsize)
            if t==1: s.segs.append((va,off,fsz,msz))
    def rd(s,va,n):
        for sva,off,fsz,msz in s.segs:
            if sva<=va<sva+msz:
                o=va-sva
                if o+n<=fsz: return s.d[off+o:off+o+n]
                return b'\0'*n
        return None
    def u32(s,va): return struct.unpack('<I',s.rd(va,4))[0]
    def f32(s,va): return struct.unpack('<f',s.rd(va,4))[0]
if __name__=='__main__':
    e=Elf(sys.argv[1]); va=int(sys.argv[2],16); n=int(sys.argv[3]) if len(sys.argv)>3 else 16
    for i in range(n):
        a=va+4*i; v=e.u32(a); print(f"{a:08x}: {v:08x}  {e.f32(a):g}")
