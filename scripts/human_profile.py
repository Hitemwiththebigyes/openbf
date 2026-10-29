#!/usr/bin/env python3
"""Card DPRX: a player's profile from a MATCH LOG, and the tuning loop that
fits the sweep's human proxy to it.

The windowed game (dev mode, on by default; OPENBF_MATCHLOG=0 turns it off)
writes one match log per battle: engine/artifacts/playtest/matches/
<YYYYmmdd-HHMMSS>_<map>.jsonl when the game runs from engine/
(engine/crates/game/src/matchlog.rs has the format). A headless round
writes the same file for its human proxy (round_to_outcome with
SWEEP_PROXY and MATCHLOG_DIR).

  human_profile.py show LOG [LOG ...] [--json] [--allow-cheats]
      the profile of each log (several: also their sum)
  human_profile.py compare HUMAN_LOG [HUMAN_LOG ...] --proxy LOG [LOG ...]
      [--tol 0.2]
      the human's summed profile against the proxy logs' summed profile,
      per metric, with the gap and OK / OFF against the tolerance
  human_profile.py calibrate HUMAN_LOG [HUMAN_LOG ...] [--seeds 4]
      [--grid skill=16,18,20 reaction=6,9] [--strength good] [--bin PATH]
      [--keep DIR] [--tol 0.2]
      the tuning loop: for every point of the grid, play the human's map at
      his tier with the proxy on his team (seeds 1..N, round_to_outcome with
      SWEEP_PROXY + MATCHLOG_DIR, 16 v 16), sum the proxy's logs, and rank the
      points by their distance to the human on the FIT metrics. Prints the
      best point and the `Strength::Good` fields to set in
      engine/crates/core/src/ai/proxy.rs.

A log with a cheat on at ANY tick (its footer's `cheats`) is refused unless
--allow-cheats (then every line says CHEATS). A log without a footer (the
game crashed before writing one is impossible: it is written whole; but a
hand-cut file) is refused too.

Metrics (one definition for a human and a proxy):
  minutes        from his first spawn to the footer's tick
  kd             kills / deaths (deaths: killed + environmental)
  kills_min      kills per minute
  acc            hits / shots (a hit: an enemy hit, one per victim per tick;
                 a shot: a press)
  sight_hit_s    median seconds from SEEING an enemy (60 m, 40 deg of his
                 aim, a clear line; again after 5 s unseen) to his first hit
                 on it; sight_hit_n engagements hit, sight_conv their share
  life_s         mean seconds per life
  taken_min      damage taken per minute alive
  dist_min       metres walked per minute on foot (samples 1/6 s)
  strafe_min / strafe_share   sideways over 1 m/s and over the forward speed
  crouch_min / crouch_share, prone_min / prone_share
FIT (the calibration's distance): kd, kills_min, acc, sight_hit_s, life_s.
"""
import argparse
import json
import os
import pathlib
import statistics
import subprocess
import sys
import tempfile

HZ = 30.0
SAMPLE = 5
FIT = ["kd", "kills_min", "acc", "sight_hit_s", "life_s"]
ROOT = pathlib.Path(__file__).resolve().parent.parent
ENGINE = ROOT / "engine"


def load(path, allow_cheats=False):
    lines = pathlib.Path(path).read_text().splitlines()
    if not lines:
        sys.exit(f"{path}: empty")
    head = json.loads(lines[0])
    if head.get("kind") != "header":
        sys.exit(f"{path}: not a match log (first line is not its header)")
    end = json.loads(lines[-1])
    if not isinstance(end, dict) or end.get("kind") != "end":
        sys.exit(f"{path}: no footer: refused (cut file?)")
    rows = [json.loads(l) for l in lines[1:-1]]
    if end.get("cheats"):
        msg = f"{path}: CHEATS WERE ON in this match: {', '.join(end['cheats'])}"
        if not allow_cheats:
            sys.exit(msg + " - refused (a calibration match must be clean; --allow-cheats to read it anyway)")
        print("!" * 72 + "\n" + msg + "\n" + "!" * 72, file=sys.stderr)
    return head, rows, end


def tally(head, rows, end):
    """The raw counts of one log."""
    u, team = head["unit"], head["team"]
    t = {"kills": 0, "deaths": 0, "shots": 0, "hits": 0, "taken": 0.0, "engage": [], "dropped": 0,
         "lives": [], "alive_ticks": 0, "foot": 0, "dist": 0.0, "strafe": 0, "strafe_starts": 0,
         "crouch": 0, "crouch_starts": 0, "prone": 0, "prone_starts": 0, "minutes": 0.0,
         "cheats": list(end.get("cheats", []))}
    spawns = [r[1] for r in rows if r[0] == "spawn"]
    ends = sorted([r[1] for r in rows if r[0] == "death"]
                  + [r[1] for r in rows if r[0] == "kill" and r[3] == u])
    first = spawns[0] if spawns else head.get("from", 0)
    t["minutes"] = max(end["tick"] - first, 0) / HZ / 60.0
    for s in spawns:
        e = next((x for x in ends if x >= s), end["tick"])
        t["lives"].append(e - s)
    t["alive_ticks"] = sum(t["lives"])
    seen = {}      # (victim, gen) -> open seen ticks
    hit_keys = set()
    for r in rows:
        k = r[0]
        if k == "shot":
            t["shots"] += 1
        elif k == "kill":
            if r[3] == u:
                t["deaths"] += 1
            elif r[2] == u and r[4] != team:
                t["kills"] += 1
        elif k == "death":
            t["deaths"] += 1
        elif k == "seen":
            seen.setdefault((r[2], r[3]), []).append(r[1])
        elif k == "hit":
            _, tick, shooter, victim, gen, vteam, dmg = r[:7]
            if victim == u:
                t["taken"] += dmg
            elif shooter == u and vteam != team and (victim, tick) not in hit_keys:
                hit_keys.add((victim, tick))
                t["hits"] += 1
                opens = [s for s in seen.get((victim, gen), []) if s <= tick]
                if opens:
                    s = max(opens)
                    t["engage"].append((tick - s) / HZ)
                    seen[(victim, gen)] = [x for x in seen[(victim, gen)] if x > tick]
    t["dropped"] = sum(len(v) for v in seen.values())
    prev = None
    strafing_until = -10
    was_c = was_p = False
    for r in rows:
        if r[0] != "s":
            continue
        _, tick, alive, x, y, z, yaw, stance, side, fwd, lx, ly, seat = r
        if not alive or seat != -1:
            prev = None
            was_c = was_p = False
            continue
        t["foot"] += 1
        if prev and tick - prev[0] <= SAMPLE:
            d = ((x - prev[1]) ** 2 + (z - prev[2]) ** 2) ** 0.5
            if d < 10.0:
                t["dist"] += d
        prev = (tick, x, z)
        if abs(side) > 1.0 and abs(side) > abs(fwd):
            t["strafe"] += 1
            if tick > strafing_until + 2 * SAMPLE:
                t["strafe_starts"] += 1
            strafing_until = tick
        c, p = stance == 1, stance == 2
        t["crouch"] += c
        t["prone"] += p
        t["crouch_starts"] += c and not was_c
        t["prone_starts"] += p and not was_p
        was_c, was_p = c, p
    return t


def add(a, b):
    out = {}
    for k, v in a.items():
        out[k] = v + b[k] if not isinstance(v, list) else v + b[k]
    return out


def profile(t):
    mins = max(t["minutes"], 1e-9)
    foot = max(t["foot"] * SAMPLE / HZ / 60.0, 1e-9)
    lives = max(len(t["lives"]), 1)
    alive_min = max(t["alive_ticks"] / HZ / 60.0, 1e-9)
    n = len(t["engage"])
    return {
        "minutes": t["minutes"], "kills": t["kills"], "deaths": t["deaths"],
        "kd": t["kills"] / max(t["deaths"], 1), "kills_min": t["kills"] / mins,
        "shots": t["shots"], "hits": t["hits"], "acc": t["hits"] / max(t["shots"], 1),
        "sight_hit_s": statistics.median(t["engage"]) if n else float("nan"),
        "sight_hit_n": n, "sight_conv": n / max(n + t["dropped"], 1),
        "lives": len(t["lives"]), "life_s": t["alive_ticks"] / HZ / lives,
        "taken_min": t["taken"] / alive_min, "dist_min": t["dist"] / foot,
        "strafe_min": t["strafe_starts"] / foot, "strafe_share": t["strafe"] / max(t["foot"], 1),
        "crouch_min": t["crouch_starts"] / foot, "crouch_share": t["crouch"] / max(t["foot"], 1),
        "prone_min": t["prone_starts"] / foot, "prone_share": t["prone"] / max(t["foot"], 1),
    }


ROWS = [
    ("minutes", "{:.1f}"), ("kills", "{}"), ("deaths", "{}"), ("kd", "{:.2f}"),
    ("kills_min", "{:.2f}"), ("shots", "{}"), ("hits", "{}"), ("acc", "{:.3f}"),
    ("sight_hit_s", "{:.2f}"), ("sight_hit_n", "{}"), ("sight_conv", "{:.2f}"),
    ("lives", "{}"), ("life_s", "{:.1f}"), ("taken_min", "{:.0f}"), ("dist_min", "{:.1f}"),
    ("strafe_min", "{:.2f}"), ("strafe_share", "{:.3f}"), ("crouch_min", "{:.2f}"),
    ("crouch_share", "{:.3f}"), ("prone_min", "{:.2f}"), ("prone_share", "{:.3f}"),
]


def summed(paths, allow):
    total, heads = None, []
    for p in paths:
        head, rows, end = load(p, allow)
        heads.append((head, end))
        t = tally(head, rows, end)
        total = t if total is None else add(total, t)
    return total, heads


def describe(p, head, end):
    w = end.get("winner")
    won = "-" if w is None else ("WON" if w == head["team"] else "lost")
    cheat = f"  CHEATS {end['cheats']}" if end.get("cheats") else ""
    return (f"{p}: {head['subject']} #{head['unit']} team {head['team']} ({head['factions'][head['team']]})"
            f"  map {head['map']} {head.get('mission', '')} {head.get('era', '')}  difficulty {head['difficulty']}"
            f"  build {head.get('build_commit', '?')}  round {won} tickets {end.get('tickets')} ({end['reason']}){cheat}")


def show(args):
    total = None
    for p in args.logs:
        head, rows, end = load(p, args.allow_cheats)
        t = tally(head, rows, end)
        total = t if total is None else add(total, t)
        pr = profile(t)
        if args.json:
            print(json.dumps({"file": p, "map": head["map"], "difficulty": head["difficulty"],
                              "subject": head["subject"], "cheats": end.get("cheats", []), **pr}))
            continue
        print(describe(p, head, end))
        for k, f in ROWS:
            print(f"  {k:13} {f.format(pr[k])}")
    if len(args.logs) > 1 and not args.json:
        pr = profile(total)
        print(f"sum of {len(args.logs)} logs:")
        for k, f in ROWS:
            print(f"  {k:13} {f.format(pr[k])}")


def gap(h, p):
    if h != h or p != p:
        return float("nan")
    return (p - h) / max(abs(h), 1e-9)


def distance(hp, pp):
    gs = [abs(gap(hp[k], pp[k])) for k in FIT]
    gs = [g for g in gs if g == g]
    return sum(g * g for g in gs) / max(len(gs), 1)


def compare(args):
    ht, _ = summed(args.human, args.allow_cheats)
    pt, _ = summed(args.proxy, True)
    hp, pp = profile(ht), profile(pt)
    print(f"{'metric':13} {'human':>9} {'proxy':>9} {'gap':>7}")
    off = 0
    for k, f in ROWS:
        g = gap(hp[k], pp[k])
        mark = ""
        if k in FIT:
            ok = g == g and abs(g) <= args.tol
            off += 0 if ok else 1
            mark = "OK" if ok else "OFF"
        gs = f"{100 * g:+6.0f}%" if g == g else "      -"
        print(f"{k:13} {f.format(hp[k]):>9} {f.format(pp[k]):>9} {gs} {mark}")
    print(f"fit metrics off by more than {100 * args.tol:.0f} %: {off} of {len(FIT)}")
    return off


def run_round(bin_path, head, strength, point, seed, out_dir):
    items = ",".join(f"{k}={v}" for k, v in point.items())
    spec = f"{head['team']}:{strength}" + (f",{items}" if items else "")
    env = dict(os.environ)
    env.update({
        "SWEEP_PROXY": spec,
        "SWEEP_DIFFICULTY": head.get("difficulty") or "normal",
        "SWEEP_THREADS": "1",
        "SWEEP_NOFILL": os.environ.get("SWEEP_NOFILL", "1"),
        "MATCHLOG_DIR": str(out_dir),
    })
    if head.get("mission", "").endswith("_con_cw.ron"):
        env["SWEEP_ERA"] = "cw"
    cmd = ["systemd-run", "--user", "--scope", "-q", "-p", "MemoryMax=16G",
           "timeout", "900", str(bin_path), "60000", str(seed), head["map"]]
    subprocess.run(cmd, env=env, cwd=ENGINE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    f = pathlib.Path(out_dir) / f"{head['map']}_s{seed}.jsonl"
    return f if f.exists() else None


def calibrate(args):
    ht, heads = summed(args.human, args.allow_cheats)
    head = heads[0][0]
    if any(h["map"] != head["map"] or h["difficulty"] != head["difficulty"] or h["team"] != head["team"]
           for h, _ in heads):
        print("note: the logs differ in map / tier / team; the loop plays the first one's", file=sys.stderr)
    hp = profile(ht)
    grid = [{}]
    for g in args.grid:
        k, vs = g.split("=", 1)
        grid = [dict(p, **{k: v}) for p in grid for v in vs.split(",")]
    bin_path = pathlib.Path(args.bin) if args.bin else ENGINE / "target/release/examples/round_to_outcome"
    if not bin_path.exists():
        sys.exit(f"{bin_path}: build it first (cargo build --release -p game --example round_to_outcome)")
    keep = pathlib.Path(args.keep) if args.keep else pathlib.Path(tempfile.mkdtemp(prefix="dprx_cal_"))
    print(f"human ({len(args.human)} logs, {hp['minutes']:.0f} min): {head['map']} {head['difficulty']} "
          f"team {head['team']}: " + "  ".join(f"{k} {hp[k]:.3g}" for k in FIT))
    ranked = []
    for point in grid:
        d = keep / ("_".join(f"{k}{v}" for k, v in point.items()) or "default")
        d.mkdir(parents=True, exist_ok=True)
        files = [f for seed in range(1, args.seeds + 1)
                 if (f := run_round(bin_path, head, args.strength, point, seed, d))]
        if not files:
            print(f"  {point}: no log written")
            continue
        pt, _ = summed(files, True)
        pp = profile(pt)
        dist = distance(hp, pp)
        ranked.append((dist, point, pp))
        print(f"  {point or 'profile as is'}: distance {dist:.3f}  "
              + "  ".join(f"{k} {pp[k]:.3g}" for k in FIT), flush=True)
    if not ranked:
        sys.exit("no point played")
    ranked.sort(key=lambda r: r[0])
    dist, point, pp = ranked[0]
    gaps = [abs(gap(hp[k], pp[k])) for k in FIT if gap(hp[k], pp[k]) == gap(hp[k], pp[k])]
    worst = max(gaps) if gaps else float("nan")
    print(f"best: {point or 'profile as is'}  distance {dist:.3f}, largest fit gap {100 * worst:.0f} %"
          f" ({'within' if worst <= args.tol else 'OUTSIDE'} the {100 * args.tol:.0f} % tolerance)")
    print("set these in Strength::Good's row (engine/crates/core/src/ai/proxy.rs), then drop PROVISIONAL:")
    names = {"skill": "skill", "reaction": "reaction_ticks", "view": "view_mult", "circles": "circles",
             "strafe": "react[0]", "roll": "react[1]", "move": "react[2]", "grenade": "grenade"}
    for k, v in point.items():
        print(f"  {names.get(k, k)} = {v}")
    print(f"logs kept in {keep}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    a = sub.add_parser("show")
    a.add_argument("logs", nargs="+")
    a.add_argument("--json", action="store_true")
    a.add_argument("--allow-cheats", action="store_true")
    a.set_defaults(fn=show)
    a = sub.add_parser("compare")
    a.add_argument("human", nargs="+")
    a.add_argument("--proxy", nargs="+", required=True)
    a.add_argument("--tol", type=float, default=0.2)
    a.add_argument("--allow-cheats", action="store_true")
    a.set_defaults(fn=compare)
    a = sub.add_parser("calibrate")
    a.add_argument("human", nargs="+")
    a.add_argument("--seeds", type=int, default=4)
    a.add_argument("--grid", nargs="*", default=["skill=16,18,20", "reaction=6,9"])
    a.add_argument("--strength", default="good")
    a.add_argument("--bin")
    a.add_argument("--keep")
    a.add_argument("--tol", type=float, default=0.2)
    a.add_argument("--allow-cheats", action="store_true")
    a.set_defaults(fn=calibrate)
    args = ap.parse_args()
    r = args.fn(args)
    sys.exit(1 if isinstance(r, int) and r > 0 else 0)


if __name__ == "__main__":
    main()
