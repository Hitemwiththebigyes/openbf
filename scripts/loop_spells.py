#!/usr/bin/env python3
"""loop_spells.py <bq dir> [--min 20] [--all] [--rows] - card RLOOP's census over the sweep's 4 Hz samples (`BOT_SAMPLES` / `<out>/botq`).
A spell: one life (unit, gen) on foot inside a 2 m box (x and z extent) for --min seconds or more.
A LOOP spell: recover + travel are 80 %+ of its samples and it has a recover sample (--all: every spell).
Per spell: map, seed, unit, start, length, centre, recover episodes (runs of beh=recover), behaviours, how it ended
(free = the life walked out of the box; end = the life's samples end in it: death, recycle or the 540 s cut).
Then per map: spells, seconds, the longest; and the episodes-at-exit table (how many recover runs a spell that got FREE had)."""
import csv, glob, os, re, collections

BOX = 2.0

def measure_file(f, minimum=20.0, all_spells=False):
    """Return RLOOP spells for one bot sample CSV; the CLI and fidelity gate share this census."""
    mm = re.fullmatch(r'(.*)_s(\d+)\.csv', os.path.basename(f))
    if not mm:
        raise ValueError(f"not a bot sample filename: {f}")
    seed = int(mm.group(2))
    spells = []
    lives = collections.defaultdict(list)
    with open(f) as source:
        for r in csv.DictReader(source):
            if r['veh'] != '-1' or r['human'] != '0': continue
            lives[(int(r['unit']), int(r['gen']))].append((float(r['t']), float(r['x']), float(r['z']), r['beh'], float(r['y'])))
    for (u, g), s in lives.items():
        i = 0; n = len(s)
        while i < n:
            lox = hix = s[i][1]; loz = hiz = s[i][2]; j = i
            while j + 1 < n and s[j+1][0] - s[j][0] < 1.0:
                x, z = s[j+1][1], s[j+1][2]
                a, b, c, e = min(lox, x), max(hix, x), min(loz, z), max(hiz, z)
                if b - a > BOX or e - c > BOX: break
                lox, hix, loz, hiz = a, b, c, e; j += 1
            dur = s[j][0] - s[i][0]
            if dur < minimum: i += 1; continue
            beh = [x[3] for x in s[i:j+1]]; cnt = collections.Counter(beh)
            rt = (cnt['recover'] + cnt['travel']) / len(beh)
            ep = sum(1 for k in range(len(beh)) if beh[k] == 'recover' and (k == 0 or beh[k-1] != 'recover'))
            loop = rt >= 0.8 and cnt['recover'] > 0
            how = 'free' if j + 1 < n and s[j+1][0] - s[j][0] < 1.0 else 'end'
            if loop or all_spells:
                spells.append((dur, seed, u, g, s[i][0], (lox+hix)/2, s[i][4], (loz+hiz)/2, ep, dict(cnt.most_common(3)), how, loop))
            i = j + 1
    return spells


def main():
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory')
    parser.add_argument('--min', type=float, default=20.0, dest='minimum')
    parser.add_argument('--all', action='store_true', dest='all_spells')
    parser.add_argument('--rows', action='store_true')
    args = parser.parse_args()
    per = collections.defaultdict(list)
    freeN = collections.Counter(); endN = collections.Counter()
    files = [p for p in glob.glob(os.path.join(args.directory, '*_s[0-9]*.csv'))
             if not p.endswith('.ev.csv')]
    for f in sorted(files):
        m = re.fullmatch(r'(.*)_s(\d+)\.csv', os.path.basename(f)).group(1)
        spells = measure_file(f, args.minimum, args.all_spells)
        if spells:
            per[m].extend(spells)
    for spells in per.values():
        for spell in spells:
            if spell[11]:
                (freeN if spell[10] == 'free' else endN)[spell[8]] += 1
    tot = 0; long30 = 0
    print('| map | spells | seconds | longest s | over 30 s | free / end | worst spot |')
    print('|---|---|---|---|---|---|---|')
    for m in sorted(per):
        v = per[m]; w = max(v); tot += len(v); long30 += sum(1 for x in v if x[0] > 30)
        print(f"| {m} | {len(v)} | {sum(x[0] for x in v):.0f} | {w[0]:.0f} | {sum(1 for x in v if x[0] > 30)} | {sum(1 for x in v if x[10]=='free')} / {sum(1 for x in v if x[10]=='end')} | s{w[1]} u{w[2]} ({w[5]:.0f},{w[6]:.0f},{w[7]:.0f}) |")
    print(f"total spells {tot}, over 30 s {long30}, seconds {sum(x[0] for v in per.values() for x in v):.0f}, files {len(files)}")
    print('recover episodes in a loop spell that got FREE :', dict(sorted(freeN.items())))
    print('recover episodes in a loop spell that ENDED    :', dict(sorted(endN.items())))
    if args.rows:
        for m in sorted(per):
            for x in sorted(per[m], reverse=True):
                print(f"{m} s{x[1]} u{x[2]} g{x[3]} t={x[4]:.1f} len={x[0]:.0f} at ({x[5]:.1f},{x[6]:.1f},{x[7]:.1f}) rec_eps={x[8]} {x[10]} loop={int(x[11])} beh={x[9]}")


if __name__ == '__main__':
    main()
