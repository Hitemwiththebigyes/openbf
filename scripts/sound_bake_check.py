#!/usr/bin/env python3
"""sound_bake_check.py [<baked root>] [--list] : the sound bake's referee (card SNDX).

Every WAV under <baked root>/*/sounds/<bank>/ against the disc, by an independent decoder: the take's raw SPU2 ADPCM
is cut from the SOUND LVL (or the game's CORE.LVL) by this file's own reader, wrapped in a VAGp header, decoded by
ffmpeg's adpcm_psx and compared with the baked file - sample rate, frame count, every frame. Also a noise measure on
each baked file alone: the share of its frames at full scale. Prints the counts; --list names every file that differs.

Needs ffmpeg and numpy; reads the discs under ROOT/extracted (read-only). Default root: engine/assets_baked.
A name is right and a time is right in every sound log; this is the check that listens to the samples.
Known result (2026-10-06, main f5990f8): 9,160 files of 34 map directories, 9,035 equal frame for frame; 125 files (6 takes)
differ only after a saturated frame (the bake keeps the clamped value as the filter's history, ffmpeg the unclamped one).
Exit code 1 on a rate or frame-count difference, a take the disc does not hold, or a file over 1 % at full scale. Exit 2 if candidates lack disc sources or no WAVs are checked."""
import collections, glob, os, re, struct, subprocess, sys, tempfile, wave
from concurrent.futures import ThreadPoolExecutor
import numpy as np

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))


def H(s):
    """The game's name hash: FNV-1a over each byte | 0x20 (analysis/re-sound-banks.md)."""
    h = 0x811C9DC5
    for c in s.encode():
        h ^= (c | 0x20)
        h = (h * 0x1000193) & 0xFFFFFFFF
    return h


T_LIST, T_BANK, T_INFO, T_DATA = H('SoundBankList'), H('SampleBank'), H('info'), H('data')
LEAF = {T_INFO, T_DATA, H('StreamList'), H('Stream')}


def chunks(m, off, end):
    while off + 8 <= end:
        tag, sz = struct.unpack_from('<II', m, off)
        if off + 8 + sz > end:
            return
        yield tag, m[off:off + 4], off + 8, sz
        off = (off + 8 + sz + 3) & ~3


def samples(m):
    """id -> (frequency, ADPCM bytes) of every real sample; the first definition wins, as in the bake."""
    out = {}

    def bank(pl, sz):
        info = data = None
        for tag, _, cpl, csz in chunks(m, pl, pl + sz):
            if tag == T_INFO:
                info = (cpl, csz)
            if tag == T_DATA:
                data = (cpl, csz)
        if not info or not data:
            return
        pairs = [struct.unpack_from('<II', m, info[0] + 8 * i) for i in range(info[1] // 8)]
        marks = [i for i, (_, v) in enumerate(pairs) if v == H('sample')]
        off = 0
        for mi, st in enumerate(marks):
            en = marks[mi + 1] if mi + 1 < len(marks) else len(pairs)
            d = {}
            for k, v in pairs[st + 1:min(en, st + 8)]:
                d.setdefault(k, v)
            if H('id') not in d or H('size') not in d or H('frequency') not in d or H('alias') in d:
                continue
            size = d[H('size')]
            out.setdefault(d[H('id')], (d[H('frequency')], m[data[0] + off:data[0] + off + size]))
            off += size + d.get(H('padding'), 0)

    def walk(off, end, depth):
        for tag, tb, pl, sz in chunks(m, off, end):
            if tag == T_BANK:
                bank(pl, sz)
            elif tag == T_LIST:
                walk(pl, pl + sz, depth + 1)
            elif tb == b'lvl_':
                walk(pl + 8, pl + sz, depth + 1)
            elif depth <= 3 and tag not in LEAF and not all(32 <= b < 127 for b in tb) and sz > 16:
                walk(pl, pl + sz, depth + 1)

    walk(8, len(m), 0)
    return out


def ffmpeg_decode(adpcm, hz, tmp):
    hdr = b'VAGp' + struct.pack('>IIII', 0x20, 0, len(adpcm), hz) + b'\0' * 12 + b'check'.ljust(16, b'\0')
    with open(tmp + '.vag', 'wb') as f:
        f.write(hdr + adpcm)
    subprocess.run(['ffmpeg', '-v', 'error', '-y', '-f', 'vag', '-i', tmp + '.vag', '-f', 's16le', '-ac', '1',
                    tmp + '.raw'], check=True)
    pcm = np.fromfile(tmp + '.raw', dtype='<i2')
    os.remove(tmp + '.vag')
    os.remove(tmp + '.raw')
    return pcm


def read_wav(p):
    w = wave.open(p, 'rb')
    return w.getframerate(), np.frombuffer(w.readframes(w.getnframes()), dtype='<i2')


def disc_files(game):
    d = {}
    lvl = os.path.join(ROOT, 'extracted', game)
    for p in glob.glob(os.path.join(lvl, 'sound', 'DATA', '_LVL_PS2', 'SOUND', '*.LVL')):
        d[os.path.basename(p)[:-4].lower()] = p
    for p in glob.glob(os.path.join(lvl, '*', 'DATA', '_LVL_PS2', 'CORE.LVL')):
        d['core'] = p
    return d


def main():
    args = [a for a in sys.argv[1:] if a != '--list']
    baked = args[0] if args else os.path.join(ROOT, 'engine', 'assets_baked')
    disc = {g: disc_files(g) for g in ('bf1', 'bf2')}
    cache, jobs, tot, bad, fatal = {}, [], collections.Counter(), [], 0
    for ron in sorted(glob.glob(os.path.join(baked, '*', 'sounds', 'sounds.ron'))):
        mapdir = os.path.dirname(os.path.dirname(ron))
        name = os.path.basename(mapdir)
        m = re.search(r'source:\s*"([^"]*)"', open(ron).read(4000))
        game = 'bf2' if 'bf2' in name else 'bf1' if 'bf1' in name else None
        man = os.path.join(mapdir, 'manifest.ron')
        if game is None and os.path.exists(man):
            g = re.search(r'source_game:\s*"?(\w+)', open(man).read(3000))
            game = g.group(1).lower() if g else None
        lvl = disc.get(game, {}).get(m.group(1) if m else '')
        wavs = sorted(glob.glob(os.path.join(mapdir, 'sounds', '*', '*.wav')))
        if lvl is None:
            tot['map dirs with no disc source found'] += 1
            tot['baked WAVs unchecked (no disc source)'] += len(wavs)
            print('NO SOURCE', name, len(wavs), 'files')
            continue
        tot['map dirs'] += 1
        if lvl not in cache:
            cache[lvl] = samples(open(lvl, 'rb').read())
        jobs += [(lvl, w, len(jobs) + i) for i, w in enumerate(wavs)]

    with tempfile.TemporaryDirectory() as tmp:
        def one(job):
            lvl, w, n = job
            sid = int(os.path.basename(w)[:-4], 16)
            whz, pcm = read_wav(w)
            clip = float(np.mean(np.abs(pcm.astype(int)) >= 32767)) if len(pcm) else 0.0
            if sid not in cache[lvl]:
                return w, 'not on the disc', True, clip
            hz, ad = cache[lvl][sid]
            ref = ffmpeg_decode(ad, hz, os.path.join(tmp, str(n)))
            if hz != whz:
                return w, 'rate %d, the disc %d' % (whz, hz), True, clip
            if len(ref) != len(pcm):
                return w, 'frames %d, ffmpeg %d' % (len(pcm), len(ref)), True, clip
            diff = np.abs(ref.astype(int) - pcm.astype(int))
            if not diff.any():
                return w, None, False, clip
            first = int(np.nonzero(diff)[0][0])
            after = first > 0 and abs(int(pcm[first - 1])) >= 32767
            return w, 'max |diff| %d from frame %d (%s)' % (
                diff.max(), first, 'after a saturated frame' if after else 'NOT after a saturated frame'), not after, clip

        with ThreadPoolExecutor(os.cpu_count() or 4) as ex:
            for w, err, is_fatal, clip in ex.map(one, jobs):
                tot['baked WAVs checked'] += 1
                if err is None:
                    tot['equal to ffmpeg, frame for frame'] += 1
                else:
                    tot['differ'] += 1
                    tot['differ only after a saturated frame'] += int(not is_fatal)
                    bad.append((w, err))
                    fatal += int(is_fatal)
                if clip > 0.01:
                    tot['over 1 % of frames at full scale'] += 1
                    bad.append((w, '%.1f %% of frames at full scale' % (100 * clip)))
                    fatal += 1
    for k, v in sorted(tot.items()):
        print('%-40s %d' % (k, v))
    print('distinct takes that differ: %d' % len({os.path.basename(w) for w, e in bad}))
    if '--list' in sys.argv:
        for w, e in bad:
            print('  ', os.path.relpath(w, baked), e)
    if not tot['baked WAVs checked']:
        print("NO COMPARISON: no baked WAVs checked")
    incomplete = tot['baked WAVs unchecked (no disc source)'] or not tot['baked WAVs checked']
    sys.exit(1 if fatal else 2 if incomplete else 0)


main()
