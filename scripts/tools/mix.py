#!/usr/bin/env python3
"""mix.py <SOUND_LOG csv | black box snapshot .json> <baked root> [--stack <name substr>=<n>] [--wav out.wav] [--law new|old|mono] : card SNDX.
Card MIXH: a take row `file|gain|pitch|left|right` (or a snapshot row with `ears`) renders in STEREO, each ear at what
the mixer was told for it (`--law new`, the default when the rows carry ears); `--law old` renders the same rows through
the port's law before card MIXH (kira's constant-power pan at 0.7 x the side, gain x level per ear, no bus; the side is
recovered from the ears: s = (R - L) / (R + L)); `--law mono` is the SNDX render below (one ear, gain x level).
--limit: the port's master limiter after the sum (card MIXH, `audio::limiter`: 2 ms lookahead, stereo-linked, ceiling
full scale, 80 ms release), with how much of the time it turns the mix down and by how much.
Per-ear figures: peak over both ears, and the share of frames with an ear over full scale (`either`), and per ear.
The port's event-sound mix rendered offline from the log's `take` rows (file|gain|pitch at its tick, 30 ticks a second:
core::TICK_HZ - until card SNDB this said 60 and every mix was rendered at twice the density of starts), or from a playtest
snapshot's `sounds` rows (card SNDB: the last 60 s the mixer was handed; same shape, so the WAV is what the player heard less
the pan, the engine loops and the music): each take resampled to
48 kHz at rate x pitch (linear), scaled by its gain and summed in mono (pan left out: the worst case of one ear).
Prints the peak against full scale, the share of frames over full scale, the seconds that hold an overload, and the
property names that start in the 0.25 s before each overloaded stretch.
--stack: start every take of a matching `start` name n times (pitch x U(0.95, 1.05) each): the per-bolt fire sound."""
import sys, csv, os, wave, collections, random
import numpy as np
OUT_HZ, TICK = 48000, 1.0 / 30.0

def read_wav(p, cache={}):
    """A take's samples, mono float. Some baked takes are Ogg Vorbis under a .wav name: those go through ffmpeg."""
    if p not in cache:
        with open(p, 'rb') as f:
            riff = f.read(4) == b'RIFF'
        if riff:
            w = wave.open(p, 'rb')
            cache[p] = (w.getframerate(), np.frombuffer(w.readframes(w.getnframes()), dtype='<i2').astype(np.float32) / 32768.0)
        else:
            import subprocess
            pcm = subprocess.run(['ffmpeg', '-v', 'error', '-i', p, '-f', 's16le', '-ac', '1', '-ar', str(OUT_HZ), '-'],
                                 check=True, capture_output=True).stdout
            cache[p] = (OUT_HZ, np.frombuffer(pcm, dtype='<i2').astype(np.float32) / 32768.0)
    return cache[p]

def limiter(mix, sr=OUT_HZ, look=0.002, rel_s=0.08, ceil=1.0):
    """audio::limiter::Limiter over the whole mix (same rule; the output is aligned, the lookahead delay removed)."""
    from numpy.lib.stride_tricks import sliding_window_view
    n = int(round(look * sr)) + 1
    peak = np.abs(mix).max(axis=0)
    need = np.where(peak > ceil, ceil / np.maximum(peak, 1e-12), 1.0).astype(np.float64)
    # target for output frame i = min of need[i .. i + n - 1] (the frame and the lookahead after it)
    padded = np.concatenate([need, np.ones(n - 1)])
    target = sliding_window_view(padded, n).min(axis=1)
    rel = np.exp(-1.0 / (rel_s * sr))
    g = np.ones_like(target)
    idx = np.nonzero(target < 1.0)[0]
    cur = 1.0; i = int(idx[0]) if len(idx) else len(target)
    while i < len(target):
        t = target[i]
        cur = min(t, (1 - rel) * t + rel * cur)
        g[i] = cur
        i += 1
        if cur > 1.0 - 1e-6 and t == 1.0:
            nxt = np.searchsorted(idx, i)
            if nxt >= len(idx): break
            i = int(idx[nxt]); cur = 1.0
    down = g < 1.0
    print('limiter: turned down %.3f %% of the time (%.2f s), under -1 dB %.3f %%, under -3 dB %.4f %%, deepest %.1f dB' % (
        100.0 * down.mean(), down.sum() / sr, 100.0 * (g < 10 ** (-1 / 20)).mean(), 100.0 * (g < 10 ** (-3 / 20)).mean(), 20 * np.log10(g.min())))
    return (mix * g).astype(np.float32)

def main():
    log, root = sys.argv[1:3]
    stack = {}; out_wav = None; law = None; limit = False
    a = sys.argv[3:]
    while a:
        if a[0] == '--stack': k, v = a[1].split('='); stack[k] = int(v); a = a[2:]
        elif a[0] == '--wav': out_wav = a[1]; a = a[2:]
        elif a[0] == '--law': law = a[1]; a = a[2:]
        elif a[0] == '--limit': limit = True; a = a[1:]
        else: raise SystemExit(__doc__)
    if log.endswith('.json'):
        import json
        snap = json.load(open(log)); nm = snap['names']
        t0 = min([r['tick'] for r in snap['sounds']] or [0])
        rows = []
        for r in snap['sounds']:
            rows.append({'tick': r['tick'] - t0, 'src': 'start', 'name': nm['sounds'][r['name']]})
            ears = r.get('ears') or [0, 0]
            tail = '|%s|%s' % tuple(ears) if any(ears) else ''
            rows.append({'tick': r['tick'] - t0, 'src': 'take', 'name': '%s|%s|%s%s' % (nm['takes'][r['take']], r['gain'], r['pitch'], tail)})
        print('snapshot %s: %d sound rows, tick %d..%d (second 0 of the mix is tick %d)' % (log, len(snap['sounds']), t0, snap['header']['tick'], t0))
        if not rows: raise SystemExit('no sound rows in this snapshot')
    else:
        rows = list(csv.DictReader(open(log)))
    last = max(int(r['tick']) for r in rows)
    has_ears = any(r['src'] == 'take' and r['name'].count('|') == 4 for r in rows)
    if law is None: law = 'new' if has_ears else 'mono'
    if law != 'mono' and not has_ears: raise SystemExit('--law %s needs take rows with ears (card MIXH)' % law)
    print('law: %s' % law)
    mix = np.zeros((2, int((last * TICK + 12.0) * OUT_HZ)), dtype=np.float32)
    rng = random.Random(1); starts = []; n = 0
    name = None
    for r in rows:
        if r['src'] == 'start': name = r['name']; continue
        if r['src'] != 'take': continue
        f = r['name'].split('|'); gain = float(f[1]); pitch = float(f[2])
        if law == 'mono': ears = (gain, 0.0)
        else:
            le, ri = float(f[3]), float(f[4])
            if law == 'new': ears = (le, ri)
            else:
                sd = (ri - le) / (ri + le) if ri + le > 0 else 0.0
                m = (0.7 * sd + 1.0) * 0.5
                ears = (gain * np.sqrt(2 * (1 - m)), gain * np.sqrt(2 * m))
        f = f[0]
        reps = 1
        for k, v in stack.items():
            if k in (name or ''): reps = v
        for i in range(reps):
            p = pitch if i == 0 else pitch * rng.uniform(0.95, 1.05)
            hz, pcm = read_wav(os.path.join(root, f))
            step = hz * max(p, 0.05) / OUT_HZ
            m = int(len(pcm) / step)
            if m <= 1: continue
            seg = np.interp(np.arange(m) * step, np.arange(len(pcm)), pcm)
            at = int(int(r['tick']) * TICK * OUT_HZ)
            n_ = len(mix[0]) - at
            mix[0, at:at + m] += seg[:n_] * ears[0]
            mix[1, at:at + m] += seg[:n_] * ears[1]
            n += 1
        starts.append((int(r['tick']), name, gain))
    if law == 'mono': mix = mix[:1]
    if limit: mix = limiter(mix)
    ear_over = np.abs(mix) > 1.0
    over = ear_over.any(axis=0)
    secs = np.unique(np.nonzero(over)[0] // OUT_HZ)
    pk = np.abs(mix).max()
    print('takes mixed %d   peak %.2f x full scale (%.1f dB)   frames over full scale %d (%.4f %% of %d s)   seconds holding an overload %d' % (
        n, pk, 20 * np.log10(max(pk, 1e-9)), int(over.sum()), 100.0 * over.mean(), mix.shape[1] // OUT_HZ, len(secs)))
    if law != 'mono':
        print('  per ear: left peak %.2f over %.4f %%   right peak %.2f over %.4f %%   (either ear: %.4f %%)' % (
            np.abs(mix[0]).max(), 100.0 * ear_over[0].mean(), np.abs(mix[1]).max(), 100.0 * ear_over[1].mean(), 100.0 * over.mean()))
    # per overloaded second: the names starting in it (and the 0.25 s before)
    who = collections.Counter(); alone = collections.Counter()
    for s in secs:
        t0, t1 = (s - 0.25) / TICK, (s + 1) / TICK
        names = collections.Counter(nm for t, nm, g in starts if t0 <= t < t1)
        for nm in names: who[nm] += 1
        if len(names) == 1: alone[next(iter(names))] += 1
    for nm, c in who.most_common(12): print('  %4d overloaded seconds hold a start of %s (%d of them hold no other name)' % (c, nm, alone[nm]))
    # the single takes: a take that is over full scale by itself
    solo = collections.Counter()
    for t, nm, g in starts:
        if g > 1.0: solo[nm] += 1
    print('  starts with gain > 1:', dict(solo))
    if out_wav:
        w = wave.open(out_wav, 'wb'); w.setnchannels(mix.shape[0]); w.setsampwidth(2); w.setframerate(OUT_HZ)
        w.writeframes((np.clip(mix.T, -1, 1) * 32767).astype('<i2').tobytes()); w.close()
main()
