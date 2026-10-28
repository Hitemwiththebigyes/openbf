# Every charge key (and OrdnanceName) in the Ghidra listings, by enclosing function: python3 -I scripts/tools/charge_keys_decomp.py analysis/SWBFRONT.ELF-decomp.c analysis/SWBFRNT2.ELF-decomp.c
# (issue #29, analysis/re-weapon-charge.md)
import sys,re
keys={'MaxPressedTime':0x2819ffa0,'NextCharge':0x8316f444,'ChargeRateLight':0xf5701d6d,'ChargeRateHeavy':0x60ddbf8e,'MaxChargeStrengthLight':0xe6c12d74,'MaxChargeStrengthHeavy':0xa61d280f,'ChargeDelayLight':0x516c9f08,'ChargeDelayHeavy':0xf303ac7b,'OrdnanceName':0xf8899c5e}
for f in sys.argv[1:]:
    lines=open(f,errors='replace').read().split('\n')
    # function starts
    fstarts=[]
    for i,l in enumerate(lines):
        m=re.match(r'^\S.*\b(FUN_[0-9a-f]{8})\s*\(',l)
        if m and not l.rstrip().endswith(';'): fstarts.append((i,m.group(1)))
    import bisect
    fi=[s[0] for s in fstarts]
    for k,h in keys.items():
        pats=[hex(h)]
        if h>=0x80000000:
            pats+=['-'+hex(0x100000000-h), str(h-0x100000000), '0xffffffff'+('%08x'%h)]
        pats+=[str(h)]
        hi=h>>16; lo=h&0xffff
        for i,l in enumerate(lines):
            for p in pats:
                if re.search(re.escape(p)+r'(?![0-9a-fx])',l):
                    j=bisect.bisect_right(fi,i)-1
                    print(f.split('/')[-1][:8],k,p,i+1,fstarts[j][1] if j>=0 else '?',l.strip()[:90])
