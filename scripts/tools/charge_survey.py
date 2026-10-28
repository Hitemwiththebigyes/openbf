# Every weapon and ordnance class with charge keys or NextCharge, from the baked classes (run from the repo root): python3 -I scripts/tools/charge_survey.py table
# (issue #29, analysis/re-weapon-charge.md)
import re,glob,sys
def fnv(s):
    h=0x811c9dc5
    for c in s.lower().encode(): h=((h^c)*0x01000193)&0xffffffff
    return h
K={n:fnv(n) for n in ['MaxPressedTime','NextCharge','ChargeRateLight','ChargeRateHeavy','MaxChargeStrengthLight','MaxChargeStrengthHeavy','ChargeDelayLight','ChargeDelayHeavy','OrdnanceName','Ordnance','Velocity','MaxDamage','ShotsPerSalvo','SalvoCount','LifeSpan','ChargeSoundPitch','ClassLabel','Gravity']}
K['ChargeHold?0x0ee66c41']=0x0ee66c41
K['count0xa4dfafc0']=0xa4dfafc0
K['pair0xa4120aa8']=0xa4120aa8
R={v:k for k,v in K.items()}
CHARGE={K[n] for n in ['MaxPressedTime','NextCharge','ChargeRateLight','ChargeRateHeavy','MaxChargeStrengthLight','MaxChargeStrengthHeavy','ChargeDelayLight','ChargeDelayHeavy']}
pat=re.compile(r'name: "([^"]*)",\s*kind: "([^"]*)",\s*base: "([^"]*)",\s*props: \[(.*?)\]',re.S)
pp=re.compile(r'\((\d+), "((?:[^"\\]|\\.)*)"\)')
db={}
for g in ['bf1','bf2']:
    db[g]={}
    for f in sorted(glob.glob(f'engine/assets_baked/classes/{g}_*.ron')):
        txt=open(f).read()
        for m in pat.finditer(txt):
            name,kind,base,props=m.groups()
            pr=[(int(a),b) for a,b in pp.findall(props)]
            key=name.lower()
            if key in db[g] and db[g][key][2]!=pr:
                db[g][key][3].append(f.split('/')[-1])
            elif key not in db[g]:
                db[g][key]=(kind,base,pr,[f.split('/')[-1]])
            else: db[g][key][3].append(f.split('/')[-1])
def chain(g,n):
    out=[];seen=set()
    while n and n.lower() in db[g] and n.lower() not in seen:
        seen.add(n.lower()); out.append(n.lower()); n=db[g][n.lower()][1]
    return out,n
mode=sys.argv[1] if len(sys.argv)>1 else 'survey'
if mode=='survey':
  for g in db:
    for n,(kind,base,pr,files) in sorted(db[g].items()):
        hs=[h for h,v in pr]
        c,root=chain(g,n)
        anc=[a for a in c[1:] if any(h in CHARGE for h,_ in db[g][a][2])]
        if any(h in CHARGE for h in hs) or anc:
            print(f'== {g} {n} kind={kind} base={base} root={root} chain={c} files={files} charge-ancestors={anc}')
            for h,v in pr:
                if h in R: print('   ',R[h],'=',v)
elif mode=='ordc':
  for g in db:
    for n,(kind,base,pr,files) in sorted(db[g].items()):
        if K['NextCharge'] in [h for h,_ in pr]:
            c,root=chain(g,n)
            print(f'== {g} {n} kind={kind} base={base} root={root} files={files}')
            for h,v in pr:
                if h in R: print('   ',R[h],'=',v)
elif mode=='show':
  g=sys.argv[2]; n=sys.argv[3].lower()
  c,root=chain(g,n); print(c,root)
  for a in c:
    print('---',a,db[g][a][0],db[g][a][1])
    for h,v in db[g][a][2]: print('   ',R.get(h,hex(h)),v)
if mode=='table':
  def eff(g,n,h):
    c,root=chain(g,n)
    for a in c:
        vals=[v for hh,v in db[g][a][2] if hh==h]
        if vals: return vals[0],a
    return None,None
  def levels(g,n):
    # own props split at NextCharge
    pr=db[g][n][2]
    lv=[{'thr':None,'ord':None,'shots':None,'cnt':None}]
    for h,v in pr:
        if h==K['NextCharge']:
            new=dict(lv[-1]); new['thr']=v; lv.append(new)
        elif h in (K['OrdnanceName'],K['Ordnance']): lv[-1]['ord']=v
        elif h==K['ShotsPerSalvo']: lv[-1]['shots']=v
        elif h==K['count0xa4dfafc0']: lv[-1]['cnt']=v
    return lv
  def ordbase(g,o):
    if not o: return '?'
    c,root=chain(g,o)
    if not c: return 'NOT-IN-BAKE('+o+')'
    nc=eff(g,o,K['NextCharge'])[0]
    vel=eff(g,o,K['Velocity'])[0]; dmg=eff(g,o,K['MaxDamage'])[0]
    return f'{root}{" [ordNextCharge="+nc+"]" if nc else ""} v={vel} dmg={dmg}'
  for g in db:
    for n,(kind,base,pr,files) in sorted(db[g].items()):
        if kind!='wpnc': continue
        mpt,src=eff(g,n,K['MaxPressedTime'])
        hasnc = any(h==K['NextCharge'] for h,_ in pr)
        anc_nc=[a for a in chain(g,n)[0][1:] if any(h==K['NextCharge'] for h,_ in db[g][a][2])]
        try: m=float(mpt) if mpt else 0
        except: m=0
        if m<=0 and not hasnc and not anc_nc: continue
        c,root=chain(g,n)
        rum=[eff(g,n,K[k])[0] for k in ['ChargeRateLight','MaxChargeStrengthLight','ChargeDelayLight','ChargeRateHeavy','MaxChargeStrengthHeavy','ChargeDelayHeavy']]
        print(f'{g} | {n} | base {base} -> engine {root} | MPT {mpt} (from {src}) | NC-ancestors {anc_nc} | rumble L {rum[0]}/{rum[1]}/{rum[2]} H {rum[3]}/{rum[4]}/{rum[5]}')
        ownlv=levels(g,n)
        if len(ownlv)==1 and not ownlv[0]['ord']:
            o=eff(g,n,K['OrdnanceName'])[0] or eff(g,n,K['Ordnance'])[0]
            ownlv[0]['ord']=o
        for l in ownlv:
            print(f'      level thr={l["thr"]} ord={l["ord"]} shots={l["shots"]} cnt={l["cnt"]} :: {ordbase(g,l["ord"])}')
