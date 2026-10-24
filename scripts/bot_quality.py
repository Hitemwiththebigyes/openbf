#!/usr/bin/env python3
"""Bot-quality numbers (card BOTM): does a bot MOVE like a soldier - one measure for the port and the PS2.

  scripts/bot_quality.py --port <dir>/<key>_s<seed>.csv ... [--ps2 <file> --ps2-format bf2|bf1] [--name <map>]
                         [--from S] [--to S] [--md <out.md>] [--json <out.json>] [--heat <out.png>]
  scripts/bot_quality.py --snapshot <black box .json> [--unit N]     # a playtest snapshot's 10 s trails
  scripts/bot_quality.py --selftest                                  # the instrument on hand-made tracks

Inputs, all brought to ONE form (a life = one unit from spawn to death, positions on a 4 Hz grid of GAME time by
linear interpolation, so a source's own rate does not shape the numbers):
  port      `BOT_SAMPLES=<dir>` of `round_to_outcome` (its `.ev.csv` beside it: shots, hits, kills);
  ps2 bf2   `lead_tmp/cont/cont_ps2.py`'s `<map>_members.csv` (live RAM, mission time, target post, order, seat);
  ps2 bf1   `lead_tmp/botm/tools/bf1_ps2.py`'s csv (live RAM, game clock, team, health, mounted);
  snapshot  the black box's `trails` (already 4 Hz).

Measures (bots on foot only; W = a 10 s window of 40 points, the black box trail's length):
  reversals        consecutive 0.25 s steps (each over 5 cm) whose directions oppose (dot < 0), per bot per W
  path / net       metres walked over metres gained, summed over all W
  staying W        W with under 4 m net; their mean path and reversals - a bot that stays put should stand
  pinball W        staying W with over 6 reversals or over 15 m of path (card JITR's bar)
  still/walk/run   share of 0.25 s steps under 0.5 m/s, 0.5 - 3.5, over 3.5
  spacing          share of unit-time with a teammate within 1.5 m / 3 m (within 2 m of height)
  contact          share of unit-time with an enemy within 20 m / 60 m
  retarget         target-post changes per bot-minute on foot (the first post of a life is not a change)
  seated           share of unit-time in a vehicle or turret seat
  idle 20 s        share of on-foot unit-time inside a spell of 20 s or more within 2 m of where it began
                   (and, where the source says so, with no enemy targeted / no fight order for the whole spell)
  map use          10 m cells: cells holding half the unit-time, the top-3 cells' share of unit-time and of deaths
  deaths / min     lives ended per minute of battle
  port only        team-kill share of kills, friend-hit share of hits, shots / s moving vs standing, hit rate,
                   steering shares (slide, fan, back, ledge), slide-side flips per W
"""
import argparse
import csv
import json
import math
import os
import statistics
import sys
from collections import Counter, defaultdict

GRID = 0.25
WIN = 40          # points: 10 s, the black box trail
MIN_STEP = 0.05   # metres: under this a step has no direction
STAY_NET = 4.0
PIN_REV, PIN_PATH = 6, 15.0
CELL = 10.0
IDLE_S, IDLE_R = 20.0, 2.0
STANDS = ("capture", "guard", "ride", "overwatch")   # the behaviours with a reason to stand (core::ai::Behaviour::stands)
MAX_GAP = 0.8     # seconds between two raw samples of one life before it is cut


# ---------- loading: {life key: [raw sample dict, by time]} ----------
def load_port(path):
    lives = defaultdict(list)
    with open(path, newline="") as fh:
        for r in csv.DictReader(fh):
            if r["human"] == "1":
                continue
            lives[(int(r["unit"]), int(r["gen"]))].append(dict(
                t=float(r["t"]), p=(float(r["x"]), float(r["y"]), float(r["z"])), team=int(r["team"]),
                seated=r["veh"] != "-1", fight=r["target"] != "-1", post=int(r.get("goal_cp") or r["cp"]),
                steer=r["steer"].split()[0],
                slide=int(r["slide"]), beh=r.get("beh")))
    return list(lives.values())


def load_ps2_bf2(path):
    """cont_ps2.py rows. A member's life runs while it has a soldier with health; a jump of over 15 m in one sample
    on foot is a respawn the sampler stepped over."""
    by = defaultdict(list)
    with open(path, newline="") as fh:
        for r in csv.DictReader(fh):
            if r["ai"] != "1":
                continue
            by[int(r["k"])].append(r)
    lives = []
    for rows in by.values():
        cur, last, ordered = [], None, False
        for r in rows:
            alive = r["ent"] not in ("0", "") and float(r["hp"] or 0) > 0
            if not cur:
                ordered = False
            # member+0x18 outlives a death and the goal re-draws it every second until the controller's first
            # think: a life's target counts from its first order (lead_tmp/cont/analyse.py's rule)
            ordered |= r["otype"] == "8" or (r["otype"] not in ("", "0", "1") and r["queued"] not in ("", "0"))
            p = (float(r["x"]), float(r["y"]), float(r["z"]))
            t = float(r["t"])
            if last is not None and t <= last["t"]:
                continue  # the emulator did not advance between two reads
            if alive and cur and (math.dist(p, cur[-1]["p"]) > 15.0 and not cur[-1]["seated"] and r["veh"] == "0"):
                lives.append(cur)
                cur = []
                ordered = False
            if alive:
                s = dict(t=t, p=p, team=int(r["team"]) - 1, seated=r["veh"] == "1", fight=r["otype"] == "2",
                         post=int(r["target"]) if ordered and r["target"] not in ("", "-1") else -1, steer="",
                         slide=0)
                cur.append(s)
                last = s
            elif cur:
                lives.append(cur)
                cur = []
        if cur:
            lives.append(cur)
    return lives


def load_ps2_bf1(path):
    """bf1_ps2.py rows: t (game clock s), comp, team, health, x, y, z, mounted, is_player. Teams 1 / 2 fight (3 =
    the map's locals); a soldier component is reused by the next life, so a life ends at health 0 or a 15 m jump."""
    by = defaultdict(list)
    with open(path, newline="") as fh:
        for r in csv.DictReader(fh):
            if r["is_player"] == "1" or r["team"] not in ("1", "2"):
                continue
            by[r["comp"]].append(r)
    lives = []
    for rows in by.values():
        cur = []
        for r in rows:
            t = float(r["t"])
            if cur and t <= cur[-1]["t"]:
                continue
            p = (float(r["x"]), float(r["y"]), float(r["z"]))
            alive = float(r["health"]) > 0
            seated = r["mounted"] == "1"
            if alive and cur and math.dist(p, cur[-1]["p"]) > 15.0 and not seated and not cur[-1]["seated"]:
                lives.append(cur)
                cur = []
            if alive:
                cur.append(dict(t=t, p=p, team=int(r["team"]) - 1, seated=seated, fight=None, post=None, steer="",
                                slide=0))
            elif cur:
                lives.append(cur)
                cur = []
        if cur:
            lives.append(cur)
    return lives


def load_events(path):
    ev = []
    if path and os.path.exists(path):
        with open(path, newline="") as fh:
            for r in csv.DictReader(fh):
                ev.append((float(r["t"]), r["kind"], int(r["a_team"]), int(r["b_team"]), float(r["x"]), float(r["z"]),
                           float(r["a_speed"])))
    return ev


# ---------- the 4 Hz grid ----------
def resample(life, t_from, t_to):
    """One life -> runs of consecutive grid samples [(gi, sample)]; a raw gap over MAX_GAP cuts the run."""
    runs, cur = [], []
    j = 0
    if not life:
        return runs
    gi = math.ceil(max(life[0]["t"], t_from) / GRID - 1e-9)
    end = min(life[-1]["t"], t_to)
    while gi * GRID <= end + 1e-9:
        t = gi * GRID
        while j + 1 < len(life) and life[j + 1]["t"] < t - 1e-9:
            j += 1
        a = life[j]
        b = life[j + 1] if j + 1 < len(life) else a
        if a["t"] > t + 1e-9:
            gi += 1
            continue
        if b["t"] - a["t"] > MAX_GAP:
            if cur:
                runs.append(cur)
                cur = []
            gi += 1
            continue
        f = 0.0 if b["t"] <= a["t"] else min(1.0, max(0.0, (t - a["t"]) / (b["t"] - a["t"])))
        # On the observation itself, categorical fields belong to that sample.
        # Between observations they retain the preceding sample's state.
        s = dict(b if abs(b["t"] - t) <= 1e-9 else a)
        s["p"] = tuple(a["p"][i] + (b["p"][i] - a["p"][i]) * f for i in range(3))
        cur.append((gi, s))
        gi += 1
    if cur:
        runs.append(cur)
    return runs


def window_stats(pts):
    """(path, net, reversals) of a list of (x, z)."""
    path, rev, prev = 0.0, 0, None
    for a, b in zip(pts, pts[1:]):
        dx, dz = b[0] - a[0], b[1] - a[1]
        d = math.hypot(dx, dz)
        path += d
        if d <= MIN_STEP:
            continue
        if prev is not None and dx * prev[0] + dz * prev[1] < 0.0:
            rev += 1
        prev = (dx, dz)
    return path, math.hypot(pts[-1][0] - pts[0][0], pts[-1][1] - pts[0][1]), rev


def pct(a, b):
    return 100.0 * a / b if b else float("nan")


def quant(v, q):
    if not v:
        return float("nan")
    v = sorted(v)
    return v[min(len(v) - 1, int(q * len(v)))]


# ---------- the measures of one run ----------
def measure(lives, events=None, t_from=0.0, t_to=1e9, heat=None):
    m = {}
    runs = []          # (life index, [(gi, s)])
    ends = []          # where and when a life ended inside the span
    for li, life in enumerate(lives):
        for r in resample(life, t_from, t_to):
            runs.append((li, r))
        if life and t_from <= life[-1]["t"] < t_to:
            ends.append(life[-1])
    if not runs:
        return m
    g0 = min(r[0][0] for _, r in runs)
    g1 = max(r[-1][0] for _, r in runs)
    span = (g1 - g0) * GRID
    # a life whose last sample is the capture's end did not die
    deaths = [e for e in ends if e["t"] < g1 * GRID - 1.0]
    m["span_s"] = span
    m["lives"] = len(lives)
    m["deaths_per_min"] = len(deaths) / (span / 60.0) if span else float("nan")

    # per-sample shares
    by_g = defaultdict(list)
    n_all = n_seat = 0
    for _, r in runs:
        for gi, s in r:
            n_all += 1
            n_seat += s["seated"]
            by_g[gi].append(s)
    m["unit_s"] = n_all * GRID
    m["seated_pct"] = pct(n_seat, n_all)
    near15 = near3 = c20 = c60 = foot = 0
    cells = Counter()
    for units in by_g.values():
        for a in units:
            if a["seated"]:
                continue
            foot += 1
            cells[(math.floor(a["p"][0] / CELL), math.floor(a["p"][2] / CELL))] += 1
            dm = de = 1e9
            for b in units:
                if b is a:
                    continue
                d = math.hypot(a["p"][0] - b["p"][0], a["p"][2] - b["p"][2])
                if b["team"] == a["team"]:
                    if not b["seated"] and abs(a["p"][1] - b["p"][1]) < 2.0:
                        dm = min(dm, d)
                else:
                    de = min(de, math.dist(a["p"], b["p"]))
            near15 += dm < 1.5
            near3 += dm < 3.0
            c20 += de < 20.0
            c60 += de < 60.0
    m["mate_1p5_pct"] = pct(near15, foot)
    m["mate_3_pct"] = pct(near3, foot)
    m["contact20_pct"] = pct(c20, foot)
    m["contact60_pct"] = pct(c60, foot)

    # map use
    tot = sum(cells.values())
    ranked = cells.most_common()
    acc, half = 0, 0
    for _, n in ranked:
        acc += n
        half += 1
        if acc * 2 >= tot:
            break
    m["cells_used"] = len(ranked)
    m["cells_half_time"] = half
    m["top3_time_pct"] = pct(sum(n for _, n in ranked[:3]), tot)
    dcell = Counter((math.floor(e["p"][0] / CELL), math.floor(e["p"][2] / CELL)) for e in deaths)
    m["top3_death_pct"] = pct(sum(n for _, n in dcell.most_common(3)), len(deaths))
    if heat is not None:
        heat["time"], heat["deaths"] = cells, dcell

    # foot runs: windows, steps, idle spells, retargets, slide flips
    wins = []
    still = walk = run = 0
    idle = idle_nofight = stand = 0
    has_fight = has_beh = False
    retarget = 0
    foot_s = 0.0
    flips = 0
    steer = Counter()
    for _, r in runs:
        seg = []
        segs = []
        for gi, s in r:
            if s["seated"]:
                if seg:
                    segs.append(seg)
                seg = []
            else:
                seg.append(s)
        if seg:
            segs.append(seg)
        # the target post over the whole run (a seat does not reset it)
        last_post = None
        for _, s in r:
            if s["post"] is None or s["seated"]:
                continue
            if s["post"] >= 0:
                if last_post is not None and s["post"] != last_post:
                    retarget += 1
                last_post = s["post"]
        for seg in segs:
            foot_s += len(seg) * GRID
            xz = [(s["p"][0], s["p"][2]) for s in seg]
            for a, b in zip(xz, xz[1:]):
                v = math.hypot(b[0] - a[0], b[1] - a[1]) / GRID
                if v < 0.5:
                    still += 1
                elif v < 3.5:
                    walk += 1
                else:
                    run += 1
            for s in seg:
                if s["steer"]:
                    steer[s["steer"]] += 1
            for i in range(0, len(seg) - WIN + 1, WIN):
                w = seg[i:i + WIN]
                path, net, rev = window_stats(xz[i:i + WIN])
                sides = [s["slide"] for s in w if s["slide"]]
                fl = sum(1 for a, b in zip(sides, sides[1:]) if a != b)
                flips += fl
                wins.append((path, net, rev, fl))
            # B9, unexplained standing (card MOVR; bot-design.md 6.3): a spell of IDLE_S or more within IDLE_R of
            # where it began, with no target all through it, in a behaviour with no reason to stand
            if seg[0].get("beh") is not None:
                has_beh = True
                n0, at = 0, None
                for k, s in enumerate(seg):
                    ok = not s["fight"] and s["beh"] not in STANDS
                    if ok and at is not None and math.hypot(xz[k][0] - at[0], xz[k][1] - at[1]) < IDLE_R:
                        n0 += 1
                        if n0 == int(IDLE_S / GRID):
                            stand += n0
                        elif n0 > int(IDLE_S / GRID):
                            stand += 1
                    else:
                        n0, at = (1, xz[k]) if ok else (0, None)
            # idle spells: the bot stays within IDLE_R of where the spell began
            i = 0
            need = int(IDLE_S / GRID)
            while i < len(seg):
                j = i + 1
                while j < len(seg) and math.hypot(xz[j][0] - xz[i][0], xz[j][1] - xz[i][1]) <= IDLE_R:
                    j += 1
                if j - i >= need:
                    idle += j - i
                    if seg[i]["fight"] is not None:
                        has_fight = True
                        if not any(s["fight"] for s in seg[i:j]):
                            idle_nofight += j - i
                    i = j
                else:
                    i += 1
    steps = still + walk + run
    m["still_pct"], m["walk_pct"], m["run_pct"] = pct(still, steps), pct(walk, steps), pct(run, steps)
    m["windows"] = len(wins)
    if wins:
        revs = [w[2] for w in wins]
        m["rev_mean"] = statistics.fmean(revs)
        m["rev_p90"] = quant(revs, 0.9)
        m["rev_max"] = max(revs)
        m["rev6_pct"] = pct(sum(1 for r in revs if r > PIN_REV), len(wins))
        m["path_net"] = sum(w[0] for w in wins) / max(1e-6, sum(w[1] for w in wins))
        stay = [w for w in wins if w[1] < STAY_NET]
        m["stay_pct"] = pct(len(stay), len(wins))
        m["stay_path"] = statistics.fmean(w[0] for w in stay) if stay else float("nan")
        m["stay_rev"] = statistics.fmean(w[2] for w in stay) if stay else float("nan")
        m["pinball_pct"] = pct(sum(1 for w in stay if w[2] > PIN_REV or w[0] > PIN_PATH), len(wins))
        if steer:
            m["flips_per_w"] = flips / len(wins)
    m["idle20_pct"] = pct(idle, foot)
    if has_beh:
        m["stand20_pct"] = pct(stand, foot)
    if has_fight:
        m["idle20_nofight_pct"] = pct(idle_nofight, foot)
    if any(s["post"] is not None for _, r in runs for _, s in r[:1]):
        m["retarget_per_min"] = retarget / (foot_s / 60.0) if foot_s else float("nan")
    if steer:
        n = sum(steer.values())
        for k in ("slide", "fan", "back", "ledge"):
            m[f"steer_{k}_pct"] = pct(steer[k], n)

    if events:
        ev = [e for e in events if t_from <= e[0] < t_to]
        kills = [e for e in ev if e[1] == "kill"]
        hits = [e for e in ev if e[1] == "hit"]
        shots = [e for e in ev if e[1] == "shot"]
        m["kills"] = len(kills)
        m["teamkill_pct"] = pct(sum(1 for e in kills if e[2] == e[3]), len(kills))
        m["friend_hit_pct"] = pct(sum(1 for e in hits if e[2] == e[3]), len(hits))
        m["hit_rate_pct"] = pct(len(hits), len(shots))
        m["shots_moving_pct"] = pct(sum(1 for e in shots if e[6] > 1.0), len(shots))
        m["shots_per_bot_min"] = len(shots) / (m["unit_s"] / 60.0)
        kc = Counter((math.floor(e[4] / CELL), math.floor(e[5] / CELL)) for e in kills)
        m["top3_kill_pct"] = pct(sum(n for _, n in kc.most_common(3)), len(kills))
    return m


# ---------- the table ----------
ROWS = [
    # key, label, format, PS2 has it
    ("rev_mean", "reversals per bot per 10 s (mean)", "{:.2f}"),
    ("rev_p90", "reversals per 10 s, 90th percentile", "{:.0f}"),
    ("rev_max", "reversals per 10 s, worst window", "{:.0f}"),
    ("rev6_pct", "windows with over 6 reversals (%)", "{:.1f}"),
    ("path_net", "path / net (m walked per m gained)", "{:.2f}"),
    ("stay_pct", "staying windows, net under 4 m (%)", "{:.1f}"),
    ("stay_path", "path in a staying window (m)", "{:.1f}"),
    ("stay_rev", "reversals in a staying window", "{:.2f}"),
    ("pinball_pct", "pinball windows (%)", "{:.2f}"),
    ("still_pct", "standing still (%)", "{:.1f}"),
    ("walk_pct", "walking 0.5-3.5 m/s (%)", "{:.1f}"),
    ("run_pct", "running over 3.5 m/s (%)", "{:.1f}"),
    ("mate_1p5_pct", "a teammate within 1.5 m (%)", "{:.1f}"),
    ("mate_3_pct", "a teammate within 3 m (%)", "{:.1f}"),
    ("contact20_pct", "an enemy within 20 m (%)", "{:.1f}"),
    ("contact60_pct", "an enemy within 60 m (%)", "{:.1f}"),
    ("retarget_per_min", "target-post changes per bot-minute", "{:.3f}"),
    ("seated_pct", "in a vehicle or turret seat (%)", "{:.1f}"),
    ("idle20_pct", "inside a 20 s spell within 2 m (%)", "{:.1f}"),
    ("idle20_nofight_pct", "...and not fighting the whole spell (%)", "{:.1f}"),
    ("stand20_pct", "B9 unexplained standing: such a spell, no target, no reason to stand (%)", "{:.2f}"),
    ("cells_used", "10 m cells walked", "{:.0f}"),
    ("cells_half_time", "cells holding half the unit-time", "{:.0f}"),
    ("top3_time_pct", "top-3 cells' share of unit-time (%)", "{:.1f}"),
    ("top3_death_pct", "top-3 cells' share of deaths (%)", "{:.1f}"),
    ("deaths_per_min", "deaths per minute", "{:.1f}"),
    ("teamkill_pct", "team kills, share of kills (%)", "{:.1f}"),
    ("friend_hit_pct", "hits on a teammate, share of hits (%)", "{:.1f}"),
    ("hit_rate_pct", "hits per shot (%)", "{:.1f}"),
    ("shots_moving_pct", "shots fired while moving over 1 m/s (%)", "{:.1f}"),
    ("shots_per_bot_min", "shots per bot-minute", "{:.1f}"),
    ("steer_slide_pct", "steering: wall slide (%)", "{:.1f}"),
    ("steer_fan_pct", "steering: fan (%)", "{:.1f}"),
    ("steer_back_pct", "steering: back (%)", "{:.1f}"),
    ("flips_per_w", "slide-side flips per 10 s", "{:.2f}"),
    ("windows", "10 s windows measured", "{:.0f}"),
    ("unit_s", "unit-seconds", "{:.0f}"),
]


def nan(x):
    return x is None or (isinstance(x, float) and math.isnan(x))


def table(name, ps2, ports, ps2_note=""):
    """Markdown: PS2 | port (the seeds' median, min .. max) | port / PS2, the rows furthest from 1 first."""
    rows = []
    for key, label, fmt in ROWS:
        pv = [p[key] for p in ports if key in p and not nan(p[key])]
        a = ps2.get(key) if ps2 else None
        if not pv and nan(a):
            continue
        med = statistics.median_low(pv) if pv else None
        ratio = None
        if not nan(a) and med is not None and key not in ("windows", "unit_s"):
            ratio = (med / a) if a else (float("inf") if med else 1.0)
        port = "-" if med is None else (fmt.format(med) + (f" ({fmt.format(min(pv))} .. {fmt.format(max(pv))})" if len(pv) > 1 else ""))
        rows.append((key, label, "-" if nan(a) else fmt.format(a), port, ratio))

    def off(r):
        if r[4] is None:
            return -1.0
        if r[4] in (0.0, float("inf")):
            return 99.0
        return abs(math.log(r[4]))
    rows.sort(key=lambda r: -off(r))
    out = [f"### {name}", "", f"PS2: {ps2_note}  port: {len(ports)} seed(s)." if ps2 else f"port: {len(ports)} seed(s).", "",
           "| measure | PS2 | port | port / PS2 |", "|---|---:|---:|---:|"]
    for _, label, a, p, r in rows:
        out.append(f"| {label} | {a} | {p} | {'-' if r is None else ('inf' if r == float('inf') else f'{r:.2f}')} |")
    return "\n".join(out) + "\n"


def write_heat(path, name, ps2h, porth):
    """Unit-time by 10 m cell, the PS2 (left) beside the port (right), each scaled to its own busiest cell (black
    = never walked, dark red .. yellow = more); a cyan dot = deaths in that cell (bigger = more). +x right, +z up."""
    try:
        from PIL import Image, ImageDraw
    except ImportError:
        return False
    sides = [(k, h) for k, h in (("PS2", ps2h), ("port", porth)) if h and h.get("time")]
    if not sides:
        return False
    xs = [c[0] for _, h in sides for c in h["time"]]
    zs = [c[1] for _, h in sides for c in h["time"]]
    x0, x1, z0, z1 = min(xs), max(xs), min(zs), max(zs)
    px = max(4, min(16, 640 // max(x1 - x0 + 1, z1 - z0 + 1)))
    w, h_px, pad, top = (x1 - x0 + 1) * px, (z1 - z0 + 1) * px, 12, 18
    img = Image.new("RGB", (len(sides) * (w + pad) + pad, h_px + top + pad), (24, 24, 28))
    d = ImageDraw.Draw(img)

    def ramp(v):
        v = v ** 0.5
        return (int(40 + 215 * min(1.0, 1.6 * v)), int(255 * max(0.0, 1.5 * v - 0.4)), int(30 * (1 - v)))
    for k, (label, hm) in enumerate(sides):
        ox = pad + k * (w + pad)
        d.rectangle((ox, top, ox + w - 1, top + h_px - 1), fill=(0, 0, 0))
        d.text((ox, 3), f"{name} {label}: unit-time by 10 m cell, deaths cyan", fill=(220, 220, 220))
        peak = max(hm["time"].values())
        for (cx, cz), n in hm["time"].items():
            x, y = ox + (cx - x0) * px, top + (z1 - cz) * px
            d.rectangle((x, y, x + px - 1, y + px - 1), fill=ramp(n / peak))
        deaths = hm.get("deaths") or {}
        dpeak = max(deaths.values(), default=1)
        for (cx, cz), n in deaths.items():
            if not (x0 <= cx <= x1 and z0 <= cz <= z1):
                continue
            r = 1 + (px // 2 - 1) * (n / dpeak) ** 0.5
            x, y = ox + (cx - x0) * px + px / 2, top + (z1 - cz) * px + px / 2
            d.ellipse((x - r, y - r, x + r, y + r), outline=(0, 230, 255))
    img.save(path)
    return True


PS2_JSON = os.path.join(os.path.dirname(os.path.abspath(__file__)), "bot_quality_ps2.json")


def seeds_section(sample_dir, t_to=540.0):
    """The sweep's section (`sweep_seeds.py --botq`): one table per map sampled into `sample_dir`, the PS2's column
    from `scripts/bot_quality_ps2.json` where that map has one. Both sides: the battle from 40 s to `t_to`."""
    try:
        ps2 = json.load(open(PS2_JSON))
    except OSError:
        ps2 = {}
    by = defaultdict(list)
    for f in sorted(os.listdir(sample_dir)):
        if f.endswith(".csv") and not f.endswith(".ev.csv") and "_s" in f:
            by[f.rsplit("_s", 1)[0]].append(os.path.join(sample_dir, f))
    out = []
    for key in sorted(by, key=lambda k: (k not in ps2, k)):
        ports = [measure(load_port(p), load_events(p[:-4] + ".ev.csv"), 40.0, t_to) for p in by[key]]
        ref = ps2.get(key)
        out += table(key, ref and ref["measures"], ports, ref["note"] if ref else "").splitlines() + [""]
    return out or ["(no bot samples)"]


# ---------- the instrument's own check ----------
def selftest():
    """Hand-made tracks: an oscillator given N reversals reads N; a straight runner reads 0 and path / net 1; a
    track sampled at an uneven 5.4 Hz reads the same as at 4 Hz."""
    ok = True

    def life(fn, hz=4.0, secs=10.0, jitter=0.0):
        out, t, i = [], 0.0, 0
        while t <= secs + 1e-9:
            x, z = fn(t)
            out.append(dict(t=t, p=(x, 0.0, z), team=0, seated=False, fight=None, post=None, steer="", slide=0,
                            beh=None))
            i += 1
            t = i / hz + (jitter * ((i * 7919) % 5 - 2) / 5.0 if i else 0.0)
        return out

    def tri(period, amp):
        # back and forth along x: a reversal every half period
        def f(t):
            ph = (t / period) % 1.0
            return (amp * (2 * ph if ph < 0.5 else 2 - 2 * ph), 0.0)
        return f

    cases = [
        ("oscillator, a turn every 0.5 s (19 in 9.75 s)", life(tri(1.0, 1.5)), 19),
        ("oscillator, a turn every 1 s (9)", life(tri(2.0, 3.0)), 9),
        ("straight runner at 6 m/s (0)", life(lambda t: (6.0 * t, 0.0)), 0),
        ("standing still with 1 cm of noise (0)", life(lambda t: (0.01 * ((int(t * 4) * 31) % 3 - 1), 0.0)), 0),
        ("oscillator a turn every 1 s, sampled at an uneven 5.4 Hz (9)", life(tri(2.0, 3.0), hz=5.4, jitter=0.04), 9),
    ]
    for label, lf, want in cases:
        runs = resample(lf, 0.0, 1e9)
        pts = [(s["p"][0], s["p"][2]) for _, s in runs[0]][:WIN]
        path, net, rev = window_stats(pts)
        good = rev == want and len(pts) == WIN
        ok &= good
        print(f"{'ok ' if good else 'BAD'} {label}: {len(pts)} points, path {path:.1f} m, net {net:.1f} m, reversals {rev}")
    m = measure([cases[2][1]])
    good = abs(m["path_net"] - 1.0) < 1e-6 and m["run_pct"] == 100.0
    ok &= good
    print(f"{'ok ' if good else 'BAD'} runner: path / net {m['path_net']:.3f}, running {m['run_pct']:.0f} %")
    # two teammates 1 m apart, an enemy 30 m off: spacing 100 %, contact20 0 %, contact60 100 %
    a = life(lambda t: (0.0, 0.0))
    b = life(lambda t: (1.0, 0.0))
    c = life(lambda t: (30.0, 0.0))
    for s in c:
        s["team"] = 1
    m = measure([a, b, c])
    good = abs(m["mate_1p5_pct"] - 200.0 / 3) < 0.01 and m["contact20_pct"] == 0.0 and m["contact60_pct"] == 100.0 \
        and m["idle20_pct"] == 0.0 and m["still_pct"] == 100.0
    ok &= good
    print(f"{'ok ' if good else 'BAD'} three standers: mate<1.5 {m['mate_1p5_pct']:.1f} % (want 66.7), "
          f"enemy<20 {m['contact20_pct']:.0f} %, <60 {m['contact60_pct']:.0f} %")
    # B9: 30 s on one spot with no target reads 100 % when the behaviour has no reason to stand, 0 % for a guard,
    # 0 % with a target; a walker reads 0 %
    def b9(beh, fight, fn=lambda t: (0.0, 0.0)):
        lf = life(fn, secs=30.0)
        for s in lf:
            s["beh"], s["fight"] = beh, fight
        return measure([lf]).get("stand20_pct")
    got = (b9("travel", False), b9("guard", False), b9("travel", True), b9("travel", False, lambda t: (6.0 * t, 0.0)))
    good = got == (100.0, 0.0, 0.0, 0.0)
    ok &= good
    print(f"{'ok ' if good else 'BAD'} B9 unexplained standing: idle traveller {got[0]:.0f} %, guard {got[1]:.0f} %, "
          f"fighter {got[2]:.0f} %, runner {got[3]:.0f} % (want 100, 0, 0, 0)")
    return 0 if ok else 1


def snapshot(path, unit):
    d = json.load(open(path))
    print(f"snapshot {path}: commit {d['header'].get('commit')} seed {d['header'].get('seed')} tick {d['header'].get('tick')}")
    rows = []
    for t in d["trails"]:
        pts = [(p[0], p[-1]) for p in t["pts"]]
        if len(pts) < 2:
            continue
        path_m, net, rev = window_stats(pts)
        rows.append((rev, t["id"], len(pts), path_m, net))
    for rev, uid, n, path_m, net in sorted(rows, reverse=True):
        if unit is None or uid == unit:
            pin = net < STAY_NET and (rev > PIN_REV or path_m > PIN_PATH)
            print(f"  unit {uid:3d}: {n} points, path {path_m:5.1f} m, net {net:5.1f} m, reversals {rev:2d}{'  PINBALL' if pin else ''}")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", nargs="*", default=[])
    ap.add_argument("--ps2")
    ap.add_argument("--ps2-format", choices=("bf2", "bf1"), default="bf2")
    ap.add_argument("--ps2-note", default="")
    ap.add_argument("--name", default="")
    ap.add_argument("--from", dest="t_from", type=float, default=0.0)
    ap.add_argument("--to", dest="t_to", type=float, default=1e9)
    ap.add_argument("--ps2-from", type=float, help="the PS2 sample's own start (default --from)")
    ap.add_argument("--ps2-json", help="add the PS2's measures to this baseline file (scripts/bot_quality_ps2.json)")
    ap.add_argument("--md")
    ap.add_argument("--json")
    ap.add_argument("--heat")
    ap.add_argument("--snapshot")
    ap.add_argument("--unit", type=int)
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        return selftest()
    if a.snapshot:
        return snapshot(a.snapshot, a.unit)
    ps2 = ps2h = None
    if a.ps2:
        ps2h = {}
        lives = (load_ps2_bf2 if a.ps2_format == "bf2" else load_ps2_bf1)(a.ps2)
        ps2 = measure(lives, None, a.t_from if a.ps2_from is None else a.ps2_from, a.t_to, ps2h)
    ports, porth = [], {}
    for i, p in enumerate(a.port):
        ports.append(measure(load_port(p), load_events(p[:-4] + ".ev.csv"), a.t_from, a.t_to, porth if i == 0 else None))
    name = a.name or (os.path.basename(a.port[0]).rsplit("_s", 1)[0] if a.port else os.path.basename(a.ps2 or ""))
    note = a.ps2_note or (f"{ps2['span_s']:.0f} s, {ps2['lives']} lives." if ps2 else "")
    text = table(name, ps2, ports, note)
    if a.md:
        with open(a.md, "w") as fh:
            fh.write(text)
    else:
        print(text)
    if a.ps2_json and ps2:
        try:
            base = json.load(open(a.ps2_json))
        except OSError:
            base = {}
        base[name] = {"note": note, "source": a.ps2, "measures": {k: v for k, v in ps2.items() if not nan(v)}}
        with open(a.ps2_json, "w") as fh:
            json.dump(base, fh, indent=1, sort_keys=True)
            fh.write("\n")
    if a.json:
        json.dump({"name": name, "ps2": ps2, "port": ports}, open(a.json, "w"), indent=1)
    if a.heat:
        write_heat(a.heat, name, ps2h, porth)
    return 0


if __name__ == "__main__":
    sys.exit(main())
