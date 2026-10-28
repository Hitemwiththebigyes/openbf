# The charge keys as data words and lui/ori builds in the ELFs: python3 -I scripts/tools/charge_keys_elf.py extracted/bf1/SWBFRONT.ELF extracted/bf2/SWBFRNT2.ELF
# (issue #29, analysis/re-weapon-charge.md)
import struct,sys
keys={'MaxPressedTime':0x2819ffa0,'NextCharge':0x8316f444,'ChargeRateLight':0xf5701d6d,'ChargeRateHeavy':0x60ddbf8e,'MaxChargeStrengthLight':0xe6c12d74,'MaxChargeStrengthHeavy':0xa61d280f,'ChargeDelayLight':0x516c9f08,'ChargeDelayHeavy':0xf303ac7b,'OrdnanceName':0xf8899c5e}
for path in sys.argv[1:]:
    d=open(path,'rb').read()
    # parse program headers
    e_phoff=struct.unpack_from('<I',d,28)[0]; n=struct.unpack_from('<H',d,44)[0]
    segs=[]
    for i in range(n):
        p_type,off,va,pa,fs,ms,fl,al=struct.unpack_from('<8I',d,e_phoff+32*i)
        if p_type==1: segs.append((off,va,fs))
    for off,va,fs in segs:
        ws=struct.unpack_from('<%dI'%(fs//4),d,off)
        for k,h in keys.items():
            for i,w in enumerate(ws):
                if w==h: print(path[-12:],k,'DATA',hex(va+4*i))
                if (w>>26)==0xf: # lui
                    rt=(w>>16)&31; imm=w&0xffff
                    for j in range(i+1,min(i+13,len(ws))):
                        x=ws[j]; op=x>>26; rs=(x>>21)&31; lo=x&0xffff
                        if rs!=rt: continue
                        if op==0xd and (imm<<16|lo)==h: print(path[-12:],k,'lui/ori',hex(va+4*i),hex(va+4*j))
                        if op==9 and ((imm<<16)+(lo-0x10000 if lo&0x8000 else lo))&0xffffffff==h: print(path[-12:],k,'lui/addiu',hex(va+4*i),hex(va+4*j))
