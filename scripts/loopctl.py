#!/usr/bin/env python3
"""loopctl.py — the loop's bookkeeping, so the prompt does not have to describe it.

Cards are the backlog: engine/artifacts/loop/cards/<ID>.md, a header of `key: value` lines between `---` fences, then
free text (Why / Evidence / Acceptance / Notes). BACKLOG.md is GENERATED from the cards — never edit it by hand.

  loopctl.py next --lane N --pid P [--lanes K]   pick + claim this session's job; prints JSON (autoloop.sh calls this)
  loopctl.py orient [--job-json FILE]             the session's briefing: its job, its card, recent journal, git state, inbox
  loopctl.py start <ID>                            worktree + symlinks + target dir for a build; card -> in-progress
  loopctl.py set <ID> key=value ...                edit card header fields (status, phase, branch, resume, value, ...)
  loopctl.py new <ID> --title T --milestone M --value V --phase P [--deps A,B] [--files a,b]
  loopctl.py release <ID|PLANNER|CLEANSE|AUDIT|MAIN>   drop a claim
  loopctl.py sweep-claims --pid P                  drop dead claims and this lane's leftovers under the claim guard
  loopctl.py claim MAIN --pid P                    take the main-checkout claim (landings, docs commits)
  loopctl.py backlog                               regenerate BACKLOG.md (also done by set/new)
  loopctl.py check                                 validate every card
  loopctl.py landed <ID> <sha>                     card -> done, counters for planner/cleanse (land.py calls this)
  loopctl.py commit-docs <path…> -m MSG            commit finished docs (analysis/…, PLAN.md, engine/CRATES.md) in main:
                                                   claims MAIN, commits exactly those paths, releases MAIN — one call
  loopctl.py finish-attempt --since T             count this unfinished session once before clean wrap-up (job from OPENBF_JOB_JSON)
  loopctl.py postmortem --since T --pid P --lane N  after a session (autoloop.sh): a claim left or a card in-progress with
                                                   no report -> a DIED journal line, the claim freed (job from OPENBF_JOB_JSON)
  loopctl.py targets [--yes]                       list (default) or delete the dead lead_tmp/*_target dirs and the cargo
                                                   targets inside a done/parked/blocked card's lead_tmp/<card>/ (next runs it)
  loopctl.py worktrees [--yes]                     list every openbf-wt tree with why it is kept or a candidate; --yes saves
                                                   each candidate's diff + untracked files to lead_tmp/worktree_sweep/<date>/
                                                   and removes it and its target (the maintainer's yes, STATUS 49; card LOOPT)
  loopctl.py self-test                             pick() over fixture cards, the target sweep over a fixture lead_tmp
  loopctl.py audit-checkpoint TARGET --missing PATH … --revision REV --reason TEXT --report PATH
                                                   remember an incomplete audit; no verdict or counter change
  loopctl.py audit-reset TARGET --reviewed-by NAME --reason TEXT
                                                   clear only this target's prerequisite hold after review
  loopctl.py audit-status                          read-only audit coverage, including prerequisite holds
  loopctl.py check-absence <doc.md> [--warn] [--base REF]  every "no class authors X / absent / never …" claim in an RE doc must carry a
                                                   `[receipt:<id>]` from `re_check.py absent` (receipts/ dir); fails (exit 1) unless
                                                   --warn; --base REF checks only the lines added since REF (commit-docs does, vs HEAD)
  loopctl.py metrics [--csv] [--out FILE]          landings per day, cards done by milestone, follow-up ratio, audit verdicts, spend per
                                                   session kind, tests: summary on stdout, tidy CSV (section,key,value) to metrics.csv
  loopctl.py scoreboard                            the numbers for STATUS.md (cards per milestone, M-ALPHA, spend, tests, sweep, audits by domain)

Card header fields:
  id, title, milestone (M1..), value (1-5, the planner's ranking), phase (re|build|measure|capture|cleanup),
  status (todo|in-progress|parked|blocked|done), deps (comma ids), files (comma paths = the build's file set),
  declares (what the landing may move: goldens=<a>,<b>; rebake=<groups>; sweep; arena; sim; ci=+N), branch, resume, created.
  Optional: tries (unfinished sessions, default 0), try_cap (positive override; re 2, other phases 3).
  Optional: alpha (yes = M-ALPHA: first at equal value), defer (post-alpha: taken only when no other card is eligible),
  acceptance (needed: never taken until the planner writes one), domain (the audit domain, else guessed from the milestone),
  first (THE MAINTAINER'S ORDER, an integer: such cards are taken before every other card, lowest number first, whatever the
  values say — lanes 1 and 2; only the maintainer or his INBOX note sets or clears it, the planner leaves it alone).
  owner (written by `start` / `set status=in-progress` when the session's claim is another name, e.g. a cleanse's CLN*
  card under CLEANSE: `<claim> <pid>`; the picker leaves the card alone while that claim is live).
  Picker guards (card LRES): an in-progress card is not resumed while its `owner` claim is live, a live land.py is on
  its branch, or `needs:` is set; a parked card is a `land` job only when its `branch:` exists — with none it is an
  ordinary card once its deps are done (its `resume:` is the first step).
  Busy files (card PKHD): in-progress cards, live claims and built parked cards hold their `files:`; a parked card
  with `needs:` set holds none until `needs:` is cleared.
  Audits: `orient` lists the audit job's targets by risk (sim/ai/vehicle/physics/collision always, render as a 1-in-3
  sample); audits.md records a `  domain:` line under every entry.
Effort: re / capture / audit -> xhigh, planner -> high, everything else -> medium.
"""
import argparse, datetime, glob, json, os, re, subprocess, sys, time
from contextlib import contextmanager

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
LOOP = os.environ.get("OPENBF_LOOP_DIR") or os.path.join(ROOT, "engine/artifacts/loop")  # override for tests
CONTROL = os.environ.get("OPENBF_CONTROL_DIR") or ROOT  # where STOP / PAUSE live
CARDS = os.path.join(LOOP, "cards")
LOCKS = os.path.join(LOOP, "locks")
STATE = os.path.join(LOOP, "state.json")
JOURNAL = os.path.join(LOOP, "JOURNAL.md")
INBOX = os.path.join(LOOP, "INBOX.md")
STATUS = os.path.join(LOOP, "STATUS.md")
def main_checkout(root=ROOT):
    """The main checkout's root, also from a worktree (the parent of the shared .git)."""
    r = subprocess.run(["git", "-C", root, "rev-parse", "--path-format=absolute", "--git-common-dir"],
                       capture_output=True, text=True)
    return os.path.dirname(r.stdout.strip()) if r.returncode == 0 else root


# Worktrees sit beside the main checkout: <parent>/openbf-wt/<branch>.
WT_BASE = os.environ.get("OPENBF_WT_BASE") or os.path.join(os.path.dirname(main_checkout()), "openbf-wt")
FIELDS = ["id", "title", "milestone", "value", "phase", "status", "deps", "files", "declares", "needs", "effort", "branch", "resume", "created"]
DRAINED = os.path.join(LOOP, "DRAINED")
MAX_NO_PROGRESS = 3  # a card taken this many sessions in a row without its file changing is auto-blocked
PHASES = ["re", "build", "measure", "capture", "cleanup"]
STATUSES = ["todo", "in-progress", "parked", "blocked", "done"]
EFFORT = {"re": "xhigh", "capture": "xhigh", "audit": "xhigh", "planner": "high"}
PLANNER_EVERY_LANDINGS, PLANNER_EVERY_SESSIONS = 8, 12
CLEANSE_EVERY_LANDINGS, AUDIT_EVERY_SESSIONS = 15, 10
LANE_PREF = {1: ["build", "cleanup", "measure", "re", "capture"], 2: ["re", "capture", "measure", "build", "cleanup"],
             3: ["re", "audit", "capture", "measure"]}
# Lane 3 (card LANE3) is the research lane: it takes the best eligible card of ITS kinds (at any value; any card only when
# none of its kinds is eligible), and the periodic audit is its job while it runs (lanes 1-2 leave the audit to it).
RESEARCH_LANE = 3
LANES_DIR = os.path.join(LOOP, "lanes")  # lane<N>.pid = the launcher's pid, written by every pick()


# ---------- cards ----------
def read_card(path):
    txt = open(path).read()
    m = re.match(r"---\n(.*?)\n---\n?(.*)", txt, re.S)
    if not m:
        raise ValueError(f"{path}: no --- header")
    h = {}
    for ln in m.group(1).splitlines():
        if ":" in ln:
            k, v = ln.split(":", 1)
            h[k.strip()] = v.strip()
    h["_body"] = m.group(2)
    h["_path"] = path
    return h


def write_card(c):
    head = "\n".join(f"{k}: {c.get(k, '')}".rstrip() for k in FIELDS)
    extra = [k for k in c if k not in FIELDS and not k.startswith("_")]
    head += "".join(f"\n{k}: {c[k]}" for k in extra)
    open(c["_path"], "w").write(f"---\n{head}\n---\n{c.get('_body', '')}")


def cards():
    out = {}
    for p in sorted(glob.glob(os.path.join(CARDS, "*.md"))):
        c = read_card(p)
        out[c["id"]] = c
    return out


def lst(v):
    return [x.strip() for x in (v or "").split(",") if x.strip()]


def file_paths(v, root=None):
    """The paths a `files:` value names, each resolved to its repo path: entries are comma-separated, a `(...)` note is
    dropped, an entry may hold several space-separated paths, and a short form (`core/src/nav.rs`, `crates/x`) that exists
    only under engine/crates/ or engine/ becomes that full path - sessions write both, and the picker compares strings."""
    root = root or ROOT
    out = []
    for e in lst(v):
        ts = re.sub(r"\([^)]*\)", " ", e).split()
        for t in ts:
            t = t.strip().removeprefix("./")
            if not re.fullmatch(r"[\w.+/-]+", t) or (len(ts) > 1 and not ("/" in t or "." in t)):  # prose words in a list
                continue
            if not os.path.exists(os.path.join(root, t)):
                t = next((q + t for q in ("engine/crates/", "engine/") if os.path.exists(os.path.join(root, q + t))), t)
            out.append(t)
    return out


def normalise_files(v, root=None):
    """`set` / `new` write a `files:` entry's short path as its full path; notes and unknown paths are kept as written."""
    root = root or ROOT
    def fix(e):
        lead = e.strip().split(" ", 1)
        t = lead[0].removeprefix("./")
        if t and not os.path.exists(os.path.join(root, t)):
            full = next((q + t for q in ("engine/crates/", "engine/") if os.path.exists(os.path.join(root, q + t))), None)
            if full:
                return " ".join([full] + lead[1:])
        return e.strip()
    return ", ".join(fix(e) for e in lst(v))


def val(c):
    try:
        return int(c.get("value") or 0)
    except ValueError:
        return 0


# M-ALPHA tags (the planner's, 2026-09-30): `alpha: yes` goes first at equal value; `defer: post-alpha` is taken only when
# no alpha or untagged card is eligible; `acceptance: needed` (no measurable acceptance yet) is never taken.
def is_alpha(c):
    return (c.get("alpha") or "").strip().lower().startswith("yes")


def first_rank(c):
    """`first: <n>` — the maintainer's order (2026-10-01): before every unnumbered card, lowest n first. None = not numbered."""
    try:
        return int((c.get("first") or "").strip())
    except ValueError:
        return None


def is_deferred(c):
    return "post-alpha" in (c.get("defer") or "").lower()


def needs_acceptance(c):
    return (c.get("acceptance") or "").strip().lower().startswith("needed")


def try_count(c):
    return int(c.get("tries") or 0)


def try_cap(c):
    return int(c.get("try_cap") or (2 if c.get("phase") == "re" else 3))


def capped(c):
    return try_count(c) >= try_cap(c)


def block_stuck(c):
    """Keep the findings intact; orient derives the planner queue from capped cards."""
    if c.get("status") != "blocked":
        c["status"] = "blocked"
        write_card(c)
        write_backlog()


def finish_attempt(job, since):
    """Count clean and dead unfinished sessions once, including wrap-up before postmortem."""
    cid = job.get("card")
    c = cards().get(cid)
    if not c or c.get("status") == "done" or job.get("job") == "land" or job.get("phase") == "land":
        return
    # A build that produced a live landing branch has a result, even before land.py runs.
    if c.get("status") == "parked" and has_branch(c):
        return
    st = state()
    # A slice landed in this session (land.py -> `landed`) is a result, even if the card was reopened for the next slice.
    if float((st.get("landed_at") or {}).get(cid) or 0) >= since:
        return
    token = job.get("attempt_id") or f"{job.get('claim')}:{since}"
    finished = st.setdefault("finished_attempts", {})
    if finished.get(cid) == token:
        return
    # A session that moved the card to its next phase (re -> build) finished its phase: a result, and a fresh count.
    if job.get("phase") and c.get("phase") and c.get("phase") != job.get("phase"):
        if try_count(c):
            c["tries"] = "0"
            write_card(c)
        finished[cid] = token
        save_state(st)
        return
    c["tries"] = str(try_count(c) + 1)
    if capped(c):
        c["status"] = "blocked"
    write_card(c)
    finished[cid] = token
    save_state(st)
    write_backlog()


def tags(c):
    return " ".join(t for t, on in ((f"first {first_rank(c)}", first_rank(c) is not None), ("alpha", is_alpha(c)), ("post-alpha", is_deferred(c)),
                                    ("needs-acceptance", needs_acceptance(c)),
                                    (f"tries {try_count(c)}/{try_cap(c)}", try_count(c) > 0)) if on)


# ---------- state / locks ----------
def state():
    try:
        return json.load(open(STATE))
    except Exception:
        return {"landings_since_planner": 0, "landings_since_cleanse": 0, "sessions_since_planner": 0,
                "sessions_since_audit": 0, "last_landed_milestone": {}}


def save_state(s):
    json.dump(s, open(STATE, "w"), indent=1)


def alive(pid, cmd=""):
    """pid is a live PROCESS (and its command line holds `cmd`, against pid reuse)."""
    try:
        p = int(pid)
    except (TypeError, ValueError):
        return False
    if p <= 0:  # os.kill(0, 0) signals our own process group and succeeds: "0" (no pid file) is not a live owner
        return False
    try:
        os.kill(p, 0)
    except PermissionError:
        pass
    except OSError:
        return False
    # kill(tid, 0) succeeds for a THREAD id too: lanes/lane3.pid named a dead launcher's pid, reused by a Discord thread,
    # and no audit was picked for 32 sessions (card LOOPT). A process is a pid whose Tgid is itself.
    try:
        m = re.search(r"^Tgid:\s*(\d+)", open(f"/proc/{p}/status").read(), re.M)
        if m and int(m.group(1)) != p:
            return False
        return not cmd or cmd in open(f"/proc/{p}/cmdline", "rb").read().decode(errors="replace")
    except OSError:
        return not cmd


CLAIM_WRITE_GRACE = 60  # s: a claim dir with no pid file younger than this is a claimer between its mkdir and its write


def holder(d):
    """The pid a lock dir holds; "0" (dead) when there is none — unless the dir is fresh, when "-1" means held."""
    try:
        return open(os.path.join(d, "pid")).read().strip() or "0"
    except OSError:
        try:
            young = time.time() - os.stat(d).st_mtime < CLAIM_WRITE_GRACE
        except OSError:
            young = False
        return "-1" if young else "0"


def held(pid):
    return pid == "-1" or alive(pid)


def locks():
    out = {}
    for d in glob.glob(os.path.join(LOCKS, "*")):
        out[os.path.basename(d)] = holder(d)
    return out


def live_locks():
    return {k: v for k, v in locks().items() if held(v)}


@contextmanager
def claim_guard():
    """Serialize claim changes on one permanent inode; never unlink this file.

    Import lazily so read-only tools can still import this module off Linux.
    """
    import fcntl
    os.makedirs(LOCKS, exist_ok=True)
    with open(os.path.join(LOCKS, ".claims.lock"), "a") as sync:
        fcntl.flock(sync, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(sync, fcntl.LOCK_UN)


def claim(name, pid, *, holders=()):
    """Acquire a dead/free claim, or transfer from an explicitly permitted holder."""
    d = os.path.join(LOCKS, name)
    with claim_guard():
        try:
            os.mkdir(d)
        except FileExistsError:
            owner = holder(d)
            if owner not in holders and held(owner):
                return False
        with open(os.path.join(d, "pid"), "w") as f:
            f.write(str(pid))
        return True


def release(name, pid=None, *, stale=False):
    """Release under the acquisition guard, optionally only for the expected owner."""
    d = os.path.join(LOCKS, name)
    with claim_guard():
        owner = holder(d)
        if pid is not None and owner != str(pid) and (not stale or held(owner)):
            return False
        try:
            os.remove(os.path.join(d, "pid"))
        except FileNotFoundError:
            pass
        try:
            os.rmdir(d)
        except OSError:
            pass
        return True


def sweep_claims(pid):
    for name in locks():
        if release(name, pid, stale=True):
            print(f"sweeping stale lock {name}")


# ---------- backlog ----------
def ranked(cs):
    st = state()
    last = st.get("last_landed_milestone", {})
    open_ = [c for c in cs.values() if c.get("status") != "done"]
    order = {cid: i for i, cid in enumerate(cs)}  # dictionary/file order as a tie-break
    # The maintainer's `first: <n>` cards in their order; then value; among equal value an `alpha: yes` card, then the milestone
    # that landed least recently (interleaving)
    return sorted(open_, key=lambda c: (first_rank(c) is None, first_rank(c) or 0, -val(c), not is_alpha(c),
                                        last.get(c.get("milestone", ""), ""), order[c["id"]]))


def write_backlog(cs=None):
    cs = cs or cards()
    r = ranked(cs)
    L = ["# BACKLOG — GENERATED by `scripts/loopctl.py backlog` from `cards/`. Do not edit; edit the cards.",
         "", f"Generated {datetime.datetime.now():%Y-%m-%d %H:%M}. Rank = the maintainer's `first: <n>` cards in their order, then value (planner's), then `alpha`, then milestone interleave; "
         f"the picker takes `post-alpha` cards only when no other is eligible and never a `needs-acceptance` one. "
         f"{len(r)} open, {sum(1 for c in cs.values() if c.get('status') == 'done')} done.", "",
         "| # | id | M | value | phase | status | tags | deps | title |", "|---|---|---|---|---|---|---|---|---|"]
    for i, c in enumerate(r, 1):
        L.append(f"| {i} | [{c['id']}](cards/{c['id']}.md) | {c.get('milestone','')} | {c.get('value','')} | {c.get('phase','')} | "
                 f"{c.get('status','') + (' · WAITS ON THE MAINTAINER' if (c.get('needs') or '').strip() else '')} | {tags(c) or '-'} | "
                 f"{c.get('deps','') or '-'} | {c.get('title','')} |")
    open(os.path.join(LOOP, "BACKLOG.md"), "w").write("\n".join(L) + "\n")


# ---------- job selection ----------
def has_branch(c):
    """The card names a branch that exists: only then is a parked card a landing (card LRES)."""
    br = (c.get("branch") or "").strip()
    return bool(br) and subprocess.run(["git", "-C", REPO, "rev-parse", "--verify", "--quiet", f"refs/heads/{br}"],
                                       capture_output=True).returncode == 0


def parked_unbuilt(c):
    """Parked with no branch (a card set aside before any build, its `resume:` the first step): an ordinary card."""
    return c.get("status") == "parked" and not has_branch(c)


def session_claim():
    """(name, pid) of the job claim this process runs under (autoloop.sh exports both), else None."""
    try:
        name = json.loads(os.environ.get("OPENBF_JOB_JSON") or "{}").get("claim")
    except ValueError:
        name = None
    pid = os.environ.get("OPENBF_LANE_PID")
    return (name, pid) if name and pid else None


def note_owner(c):
    """A card put in progress by a session whose claim is another name (a cleanse's CLN* card under CLEANSE, a planner's,
    an audit's) records `owner: <claim> <pid>`: the picker leaves it alone while that claim is live (card LRES)."""
    sc = session_claim()
    if sc and c.get("status") == "in-progress" and sc[0] != c["id"] and c["id"] not in live_locks():
        c["owner"] = f"{sc[0]} {sc[1]}"  # not over a card whose own session is live: a planner's edit is not a take-over


def owner_live(c, live):
    name, _, pid = (c.get("owner") or "").strip().partition(" ")
    return bool(name) and live.get(name) == pid.strip()


def landing_live(c):
    """A live land.py is working this card's branch (lead_tmp/<branch>/land/state.json, running, its pid a land.py)."""
    br = (c.get("branch") or "").strip()
    if not (br and LEAD_TMP):
        return False
    try:
        st = json.load(open(os.path.join(LEAD_TMP, br, "land", "state.json")))
        return bool(st.get("running")) and b"land.py" in open(f"/proc/{int(st.get('pid'))}/cmdline", "rb").read()
    except (OSError, ValueError, TypeError):
        return False


def eligible(c, cs, busy_files):
    if c.get("status") not in ("todo", "in-progress") and not parked_unbuilt(c):
        return False
    if (c.get("needs") or "").strip():  # waiting on the maintainer: out of rotation, never a reason to stop the rest
        return False
    if capped(c):
        return False
    if needs_acceptance(c):  # no measurable acceptance yet: the planner writes one (a refill), a worker never guesses it
        return False
    for d in lst(c.get("deps")):
        if d in cs and cs[d].get("status") != "done":
            return False
    return not (set(file_paths(c.get("files"))) & busy_files)


def note_lane(lane, pid):
    os.makedirs(LANES_DIR, exist_ok=True)
    open(os.path.join(LANES_DIR, f"lane{lane}.pid"), "w").write(str(pid))


LAUNCHER = os.environ.get("OPENBF_LAUNCHER_MATCH") or "autoloop.sh"  # a lane file's pid must be this launcher (self-test overrides)


def lane_alive(lane):
    try:
        return alive(open(os.path.join(LANES_DIR, f"lane{lane}.pid")).read().strip(), LAUNCHER)
    except OSError:
        return False


def pick(lane, lanes, pid):
    """Returns the job dict and claims it. Order: stop/pause, resume, land, periodic, best card, planner."""
    note_lane(lane, pid)
    for f in ("STOP", "PAUSE"):
        if os.path.exists(os.path.join(CONTROL, f)):
            return {"job": "none", "reason": f"{f} file present"}
    if os.path.exists(DRAINED):
        return {"job": "stop", "reason": "DRAINED: " + open(DRAINED).read().strip()[:400]}
    cs = cards()
    live = live_locks()
    st = state()
    audit_wait = ""
    for c in cs.values():
        if (c.get("status") != "done" and capped(c) and c["id"] not in live
                and not owner_live(c, live) and not landing_live(c)
                and not (c.get("status") == "parked" and has_branch(c))):
            block_stuck(c)
    busy = set()
    for c in cs.values():
        if (c.get("status") == "in-progress"
                or (c.get("status") == "parked" and not (c.get("needs") or "").strip()
                    and not parked_unbuilt(c)) or c["id"] in live):
            busy |= set(file_paths(c.get("files")))

    def take(job, cid=None, phase=None, reason=""):
        name = cid or job.upper()
        if cid and job != "land":
            fp = __import__("hashlib").sha1(open(cs[cid]["_path"], "rb").read()).hexdigest()
            att = st.setdefault("attempts", {})
            a = att.get(cid, {"n": 0, "fp": ""})
            a = {"n": a["n"] + 1, "fp": fp} if a["fp"] == fp else {"n": 1, "fp": fp}
            if a["n"] > MAX_NO_PROGRESS:
                c = cs[cid]
                c["status"] = "blocked"
                c["resume"] = (f"AUTO-BLOCKED {datetime.date.today()}: taken {MAX_NO_PROGRESS} sessions in a row with no change to "
                               f"this card — the planner decides (split it, re-scope it, or mark `needs:`). " + (c.get("resume") or ""))[:600]
                write_card(c); write_backlog()
                open(JOURNAL, "a").write(f"- AUTO-BLOCKED {cid}: {MAX_NO_PROGRESS} sessions without progress\n")
                att.pop(cid, None); save_state(st)
                return None
            att[cid] = a
        if not claim(name, pid):
            return None
        eff = EFFORT.get(job if job in ("planner", "audit") else (phase or ""), "medium")
        if cid and cs[cid].get("effort") in ("low", "medium", "high", "xhigh", "max"):  # a card's own `effort:` (the RE experiment)
            eff = cs[cid]["effort"]
        st["last_job"] = job
        st["sessions_since_planner"] = st.get("sessions_since_planner", 0) + 1
        st["sessions_since_audit"] = st.get("sessions_since_audit", 0) + 1
        if job == "planner":
            st["landings_since_planner"] = 0; st["sessions_since_planner"] = 0
        if job == "cleanse":
            st["landings_since_cleanse"] = 0
        if job == "audit":
            st["sessions_since_audit"] = 0
        save_state(st)
        return {"job": job, "card": cid, "phase": phase, "effort": eff, "claim": name, "lane": lane,
                "reason": f"{reason}; {audit_wait}" if audit_wait else reason,
                "attempt_id": str(time.time_ns())}

    # 1. resume: a card in progress whose claim is not live — and that nobody else is working (LRES): not the card of a
    #    live CLEANSE / PLANNER / AUDIT session (`owner:`), not a branch a live land.py is on, not one waiting on the maintainer
    for c in ranked(cs):
        if (c.get("status") == "in-progress" and c["id"] not in live and not capped(c) and not needs_acceptance(c)
                and not (c.get("needs") or "").strip() and not owner_live(c, live) and not landing_live(c)):
            j = take("resume", c["id"], c.get("phase"), "in-progress with no live claim")
            if j:
                return j
    # 2. land a parked card that has a branch (needs MAIN free too; land.py takes MAIN itself). A parked card with no
    #    branch is an ordinary card of step 4, once its deps are done. Not one waiting on the maintainer: SKIPC's land was re-picked
    #    two minutes after its `needs:` was set (LOOPT)
    if "MAIN" not in live:
        for c in ranked(cs):
            if (c.get("status") == "parked" and c["id"] not in live and has_branch(c) and not landing_live(c)
                    and not (c.get("needs") or "").strip()):
                j = take("land", c["id"], "land", "parked, MAIN free")
                if j:
                    return j
    # 3. periodic jobs (one lane at a time; the claim keeps two lanes from both taking one)
    n_open = sum(1 for c in cs.values() if c.get("status") in ("todo", "in-progress"))
    # a card the fidelity gate refused waits for the planner (re-take the baseline or an approval): do not leave it for 8 landings
    # ... unless it waits on the maintainer (`needs:` set): the planner has judged it and has nothing new to do until he answers (CLSE2B called
    # four planners in twelve hours, 2026-10-12)
    fid_blocked = any(c.get("status") == "blocked" and not (c.get("needs") or "").strip()
                      and any(w in (c.get("resume") or "").lower() for w in ("fidelity", "exposed-claim")) for c in cs.values())
    if st.get("last_job") not in ("planner", "planner-refill") and (
            st.get("landings_since_planner", 0) >= PLANNER_EVERY_LANDINGS or st.get("sessions_since_planner", 0) >= PLANNER_EVERY_SESSIONS
            or (fid_blocked and st.get("sessions_since_planner", 0) >= 3)
            or n_open < 5 or not os.path.exists(STATUS)):
        j = take("planner", None, None, "planner due")
        if j:
            return j
    if st.get("landings_since_cleanse", 0) >= CLEANSE_EVERY_LANDINGS:
        j = take("cleanse", None, None, f"{st['landings_since_cleanse']} landings since the last cleanse")
        if j:
            return j
    if st.get("sessions_since_audit", 0) >= AUDIT_EVERY_SESSIONS and (lane == RESEARCH_LANE or not lane_alive(RESEARCH_LANE)):
        audit_wait = audit_blocked_reason(cs)
        if not audit_wait:
            j = take("audit", None, None, f"{st['sessions_since_audit']} sessions since the last audit")
            if j:
                return j
    # 4. the best eligible card, preferring this lane's kinds when there are two lanes; `post-alpha` cards only when no
    #    alpha or untagged card is eligible
    r = [c for c in ranked(cs) if c["id"] not in live and eligible(c, cs, busy) and not owner_live(c, live) and not landing_live(c)]
    r = [c for c in r if not is_deferred(c)] or r
    if lane == RESEARCH_LANE:  # its own kinds first at ANY value (a lane started with LANE=3 runs with --lanes 1)
        r = [c for c in r if c.get("phase") in LANE_PREF[lane]] + [c for c in r if c.get("phase") not in LANE_PREF[lane]]
    elif lanes > 1 and r:
        pref = LANE_PREF.get(lane, PHASES)
        numbered = [c for c in r if first_rank(c) is not None]  # the maintainer's order: ahead of the lane split, in their order
        r = [c for c in r if first_rank(c) is None]
        if r:
            best_val = val(r[0])
            top = [c for c in r if val(c) == best_val]  # lane preference splits kinds among EQUAL value only: value is the rank
            top.sort(key=lambda c: (not is_alpha(c), pref.index(c.get("phase")) if c.get("phase") in pref else 9))  # alpha still first
            r = top + [c for c in r if c not in top]
        r = numbered + r
    for c in r:
        j = take("card", c["id"], c.get("phase"), f"top eligible card (value {c.get('value')})")
        if j:
            return j
    if audit_wait:
        return {"job": "none", "reason": audit_wait}
    # nothing eligible: one refill planner; if the last job was already an empty-handed refill, the loop is drained
    if st.get("last_job") == "planner-refill":
        msg = (f"{datetime.datetime.now():%Y-%m-%d %H:%M}: two refill planners in a row found no eligible card "
               f"(open: {sum(1 for c in cs.values() if c.get('status') in ('todo', 'in-progress'))}, waiting on the maintainer: "
               f"{sum(1 for c in cs.values() if (c.get('needs') or '').strip() and c.get('status') != 'done')}, "
               f"blocked: {sum(1 for c in cs.values() if c.get('status') == 'blocked')}). See STATUS.md and BACKLOG.md.")
        open(DRAINED, "w").write(msg + "\n")
        return {"job": "stop", "reason": "DRAINED: " + msg}
    j = take("planner", None, None, "nothing eligible — refill")
    if j:
        st["last_job"] = "planner-refill"; save_state(st)
    return j or {"job": "none", "reason": "nothing claimable (another lane holds the planner)"}


# ---------- audit by risk (the maintainer, 2026-09-29 item 3) ----------
# Every audit of vehicle / behaviour research found a problem; rendering research is already checked by PS2 pixel pairs.
# So: always audit the risk domains; audit rendering only when its pair check failed, it touched shared code, or as a
# 1-in-3 sample. audits.md records a `  domain: <d>` line under every `- AUDIT` entry so the rate can be re-checked.
AUDITS = os.path.join(LOOP, "audits.md")
AUDIT_HOLDS = os.path.join(LOOP, "audit_holds.json")
AUDIT_ROOT = os.environ.get("OPENBF_AUDIT_ROOT") or main_checkout()
ANALYSIS = os.path.join(AUDIT_ROOT, "analysis")
RISK_DOMAINS = ("sim", "ai", "vehicle", "physics", "collision", "behaviour")
DOMAIN_WORDS = [  # first match wins: the risk domains before render, so "vehicle lighting" is a vehicle
    ("collision", r"collision|walls?|hull|push-?outs?|clip"),
    ("vehicle", r"vehicles?|hover\w*|turrets?|walkers?|tauntaun|speeder|skiff|flyers?|veh\w*|seats?|pilot"),
    ("ai", r"ai|bots?|hub|path\w*|commander|squad|goto"),
    ("physics", r"physics|jumps?|gravity|fall|velocity|momentum|sprint|ragdoll|corpse"),
    ("sim", r"sim|damage|health|ordnance|weapons?|grenades?|mines?|score|reinforcements?|time-?limit|capture|posts?|"
            r"aim|spawn\w*|round|death|kill\w*|ammo|powerups?|conquest|mission"),
    ("audio", r"sounds?|audio|music|voice|snd\w*"),
    ("render", r"light\w*|fog|water|sky|textures?|terrain|foliage|leaf\w*|fx|particle\w*|shader|alpha|colou?r|glow|"
               r"specular|hud|bloom|shadow\w*|render\w*|draw|golden\w*|picture|gpu|wgsl|lod|sprite|decal|bolt"),
    ("tooling", r"loop\w*|gates?|land\.py|scripts?|bake|ci|cleanse|targets?"),
]


def guess_domain(text, milestone=""):
    """A keyword guess (shown with a `?`). The milestone is the prior: M1 (Looks right) is render and M2 (Vehicles) is
    vehicle; an M0 card is read for tooling words first; M3 (Plays right) falls back to sim."""
    if milestone in ("M1", "M2"):
        return {"M1": "render", "M2": "vehicle"}[milestone]
    t = text.lower()
    words = sorted(DOMAIN_WORDS, key=lambda dp: {"tooling": 0, "render": 1}.get(dp[0], 2)) if milestone == "M0" else DOMAIN_WORDS
    for d, pat in words:
        if re.search(rf"\b(?:{pat})\b", t):
            return d
    return "sim" if milestone == "M3" else "other"


def audit_log(text):
    """audits.md -> [(target, verdict, domain)] in file order; domain '?' when an entry has no `domain:` line yet."""
    out = []
    for ln in text.splitlines():
        m = re.match(r"- AUDIT \S+ (.+?): (OK|WRONG|PARTLY)\b", ln)
        if m:
            out.append([m.group(1), m.group(2), "?"])
            continue
        m = re.match(r"\s+domain:\s*(\S+)", ln)
        if m and out and out[-1][2] == "?":
            out[-1][2] = m.group(1).lower()
    return [tuple(x) for x in out]


def audited_ids(log):
    return {t for target, _, _ in log for t in re.findall(r"[A-Z][A-Z0-9]+", target) if t != "RE"}


def render_sampled(log):
    """The 1-in-3 sample: a render audit is due when neither of the last two audits was one."""
    return not any(d == "render" for _, _, d in log[-2:])


def audit_sources(cs, log):
    """Unaudited targets, newest first: [(date, target, domain, guessed?, what)] — landed cards and `[O] UNAUDITED` RE docs."""
    done_ids, out = audited_ids(log), []
    for c in cs.values():
        m = re.match(r"landed (\w+)(?:\s+(\d{4}-\d\d-\d\d))?", c.get("resume") or "")
        if c.get("status") != "done" or not m or c["id"] in done_ids:
            continue
        date = m.group(2) or run(f"git show -s --format=%cs {m.group(1)} 2>/dev/null") or c.get("created", "")
        d = (c.get("domain") or "").strip().lower()
        out.append((date, c["id"], d or guess_domain(f"{c.get('title', '')} {c.get('files', '')}", c.get("milestone", "")), not d,
                    f"landing {m.group(1)[:8]}: {c.get('title', '')[:110]}"))
    for p in glob.glob(os.path.join(ANALYSIS, "re-*.md")):
        head = open(p).read(3000)
        m = re.match(r"# OPUS RE \((\d{4}-\d\d-\d\d)\) - \[O\] UNAUDITED", head)
        if not m or re.search(r"\bAUDIT 20\d\d-", head):
            continue
        name = os.path.basename(p)
        out.append((m.group(1), name, guess_domain(name[3:-3].replace("-", " ")), True, f"[O] RE {name}"))
    return sorted(out, key=lambda x: x[0], reverse=True)


def audit_holds():
    """Separate incomplete checkpoints from earned audit verdicts."""
    try:
        with open(AUDIT_HOLDS) as f:
            return json.load(f)
    except FileNotFoundError:
        return {}


def write_audit_holds(holds):
    # Caller holds claim_guard; replace atomically so read-only briefings never see a partial JSON write.
    path = AUDIT_HOLDS + ".tmp"
    with open(path, "w") as f:
        json.dump(holds, f, indent=2, sort_keys=True)
        f.write("\n")
    os.replace(path, AUDIT_HOLDS)


def audit_checkpoint(target, missing, revision, reason, report):
    """Record only input existence, never an audit verdict or an attempt/cadence counter."""
    if not all(x.strip() for x in [target, revision, reason, report]) or not missing:
        raise ValueError("target, missing prerequisites, revision, reason and report are required")
    if not os.path.isfile(os.path.join(AUDIT_ROOT, report)):
        raise ValueError(f"checkpoint report does not exist: {report}")
    paths = list(dict.fromkeys(missing))
    if any(not p.strip() or os.path.exists(os.path.join(AUDIT_ROOT, p)) for p in paths):
        raise ValueError("--missing must name exact paths that are currently absent")
    with claim_guard():
        holds = audit_holds()
        # A later checkpoint cannot silently drop earlier requirements; changing scope needs a reviewed reset.
        paths = list(dict.fromkeys(holds.get(target, {}).get("missing", []) + paths))
        holds[target] = dict(target=target, missing=paths, revision=revision, reason=reason, report=report,
                             recorded=datetime.datetime.now(datetime.timezone.utc).isoformat())
        write_audit_holds(holds)


def audit_reset(target, reviewed_by, reason):
    if not reviewed_by.strip() or not reason.strip():
        raise ValueError("reviewed reset requires --reviewed-by and --reason")
    with claim_guard():
        holds = audit_holds()
        if target not in holds:
            raise ValueError(f"no audit hold for {target}")
        del holds[target]
        write_audit_holds(holds)
        open(JOURNAL, "a").write(f"- AUDIT RESET {target}: reviewed by {reviewed_by} — {reason}\n")


def audit_hold_reason(target, holds):
    h = holds.get(target)
    if not h:
        return ""
    missing = [p for p in h["missing"] if not os.path.exists(os.path.join(AUDIT_ROOT, p))]
    if not missing:
        return ""
    return (f"INCOMPLETE / UNAUDITED: {h['reason']}; restore ALL: {', '.join(missing)} "
            f"(checkpoint {h['revision']}, report {h['report']}); or reviewed reset: "
            f"loopctl.py audit-reset {target} --reviewed-by NAME --reason TEXT")


def audit_candidates(sources, log, holds=None):
    """[(target, domain, why, pick?)]: the risk domains first (always audited), other domains next, render last —
    render picked only when sampled; otherwise listed for its two exceptions (a failed pair check, shared code)."""
    sampled = render_sampled(log)
    holds = audit_holds() if holds is None else holds
    rank = lambda d: 0 if d in RISK_DOMAINS else 2 if d == "render" else 1
    out = []
    for date, target, d, guessed, what in sorted(sources, key=lambda x: rank(x[2])):  # stable: newest first within a rank
        dom = d + ("?" if guessed else "")
        blocked = audit_hold_reason(target, holds)
        if blocked:
            out.append((target, dom, f"{date} {what} — SKIP {blocked}", False))
        elif d == "render" and not sampled:
            out.append((target, dom, f"{date} {what} — SKIP unless its PS2 pair check failed or it touched shared code", False))
        else:
            why = "risk domain: always audited" if d in RISK_DOMAINS else "render: the 1-in-3 sample is due" if d == "render" else "not render"
            out.append((target, dom, f"{date} {what} — {why}", True))
    return out


def audit_rows(cs=None):
    log = audit_log(open(AUDITS).read() if os.path.exists(AUDITS) else "")
    return log, audit_candidates(audit_sources(cards() if cs is None else cs, log), log)


def audit_blocked_reason(cs=None):
    _, rows = audit_rows(cs)
    if rows and not any(ok for *_, ok in rows) and any("INCOMPLETE / UNAUDITED" in why for _, _, why, _ in rows):
        return "Incomplete audit coverage; no ready pick. " + " | ".join(f"{t}: {why}" for t, _, why, _ in rows)
    return ""


def audit_briefing():
    log, cands = audit_rows()
    L = ["", "## Audit targets (audit by risk — pick the FIRST `pick` row unless you can show a skipped render row's pair check "
         "failed or it touched shared code)",
         f"Render sample due: {'yes' if render_sampled(log) else 'no'} (last two audits: "
         f"{', '.join(d for _, _, d in log[-2:]) or 'none'}). A `?` domain is a keyword guess — record the real one.",
         "A pick row preserves selection order; verify its primary inputs before auditing. Restored path existence is not a verdict.",
         "Write the verdict as `- AUDIT <date> <target>: OK|PARTLY|WRONG — …` followed by a line `  domain: <d>` "
         f"(one of {', '.join(RISK_DOMAINS)}, render, audio, tooling, other).", ""]
    if cands and not any(ok for *_, ok in cands):
        L += ["INCOMPLETE COVERAGE: no ready pick; no audit verdict earned. Restore prerequisites or review a named reset.", ""]
    # Every prerequisite hold remains visible, even beyond the ordinary render sample's four skipped rows.
    rows = ([x for x in cands if x[3]][:12]
            + [x for x in cands if not x[3] and "INCOMPLETE / UNAUDITED" in x[2]]
            + [x for x in cands if not x[3] and "INCOMPLETE / UNAUDITED" not in x[2]][:4])
    L += [f"- {'pick' if ok else 'skip'} `{t}` [{d}] {why}" for t, d, why, ok in rows]
    return L


CARD_RULES = [  # the maintainer approved 2026-09-29 (INBOX item 6); orient prints them to every session
    "- A research pass opens at most 2 new cards, unless a new card is value >= 4 or blocks a landed card; further findings "
    "go as notes on existing cards.",
    "- Every card states a measurable acceptance (a parity delta, a leak-census count, a sim replay hash, a pixel test); "
    "a card without one gets `acceptance: needed` and is not picked until the planner writes it.",
    "- Picker: value first; at equal value `alpha: yes` goes first; `defer: post-alpha` only when no alpha or untagged card "
    "is eligible; `acceptance: needed` never.",
]


# ---------- orient ----------
def run(cmd):
    return subprocess.run(cmd, shell=True, cwd=ROOT, capture_output=True, text=True).stdout.rstrip()


def orient(job):
    out = ["# Orientation (scripts/loopctl.py orient)", ""]
    out.append(f"**Your job:** `{json.dumps(job)}`")
    cs = cards()
    if job.get("card") and job["card"] in cs:
        c = cs[job["card"]]
        out += ["", f"## Your card — {c['_path'].replace(ROOT + '/', '')}", "```", open(c["_path"]).read().rstrip(), "```"]
        if job.get("job") != "land":
            out += ["", f"Try {try_count(c) + 1} of {try_cap(c)}: previous tries found: {c.get('resume') or '(none recorded)'}"]
    out += ["", "## Card rules (the maintainer, 2026-09-29)", *CARD_RULES]
    if job.get("job") == "planner":
        out += ["", "## Stuck cards — planner action required",
                "Re-cut smaller, cut a prerequisite RE card, file a Question for the maintainer in STATUS.md, or close as value 1 / defer post-alpha. "
                "Reset tries only after one of these actions, noting the action in the card."]
        out += [f"- STUCK: {c['id']} after {try_count(c)} tries: {c.get('resume') or '(no findings recorded)'}"
                for c in cs.values() if c.get("status") != "done" and capped(c)
                and not (c.get("status") == "parked" and has_branch(c))]
    if job.get("job") in ("audit", "planner"):
        out += audit_briefing()
    out += ["", "## Live claims", run(f"ls {LOCKS} 2>/dev/null") or "(none)"]
    out += ["", "## git", "```", run("git log --oneline -6"), run("git status --short | head -15"),
            run("git worktree list"), "```"]
    ib = open(INBOX).read().strip() if os.path.exists(INBOX) else ""
    if ib:
        # The OPEN notes, newest first (the file's own order). `ib[-4000:]` showed the oldest handled notes instead.
        m = re.search(r"^## Open\n(.*?)(?=^## Done|\Z)", ib, re.S | re.M)
        open_notes = (m.group(1) if m else ib).strip()
        if len(open_notes) > 6000:
            open_notes = open_notes[:6000] + "\n  […older open notes: read INBOX.md]"
        out += ["", "## the maintainer's INBOX — open notes, newest first (read them; the planner files them into cards)", open_notes]
    if os.path.exists(JOURNAL):
        tail = open(JOURNAL).read().splitlines()[-25:]
        out += ["", "## Journal tail", "```", *[l[:300] for l in tail], "```"]
    out += ["", "## Backlog top 10", *open(os.path.join(LOOP, "BACKLOG.md")).read().splitlines()[4:16]] if os.path.exists(
        os.path.join(LOOP, "BACKLOG.md")) else []
    return "\n".join(out)


# ---------- docs commits and dead sessions (LDOC) ----------
# An RE doc left uncommitted in the main checkout blocks every other lane's land.py (TALPH waited ~30 min), so a
# finished doc goes in with ONE call that takes MAIN, commits exactly the named paths and releases MAIN again.
REPO = os.environ.get("OPENBF_REPO") or ROOT  # override for tests
DOC_PATHS = ("analysis/", "PLAN.md", "engine/CRATES.md")
ATTRIBUTION = "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"


def git(*args, repo=None):
    return subprocess.run(["git", "-C", repo or REPO, *args], capture_output=True, text=True)


def doc_path(p, repo):
    """The repo-relative path if `p` is a doc commit-docs may commit (under analysis/, PLAN.md, engine/CRATES.md), else None."""
    rel = os.path.relpath(os.path.abspath(p if os.path.isabs(p) else os.path.join(repo, p)), repo)
    if rel.startswith("..") or os.path.islink(os.path.join(repo, rel)):
        return None
    return rel if rel in DOC_PATHS or rel.startswith("analysis/") else None


def commit_docs(paths, msg, pid):
    """Returns (exit code, message). Refuses a non-doc path, a detached HEAD, an index with staged changes, a path with
    nothing to commit, or MAIN held by a live session; otherwise claims MAIN, commits exactly `paths`, releases MAIN.
    With an analysis/re-*.md among them the generated analysis/INDEX.md is regenerated and rides in the same commit
    (nothing else regenerates it, and PLAN sends every RE session to grep it first)."""
    rels = [doc_path(p, REPO) for p in paths]
    bad = [p for p, r in zip(paths, rels) if r is None]
    if bad:
        return 2, f"refused: not a doc path ({', '.join(bad)}) — commit-docs takes only analysis/…, PLAN.md, engine/CRATES.md"
    br = git("symbolic-ref", "-q", "--short", "HEAD")
    if br.returncode:
        return 2, "refused: HEAD is detached"
    if br.stdout.strip() != "main":  # run from a card's worktree, REPO is that checkout: its branch is not main
        return 2, f"refused: {REPO} is on {br.stdout.strip()}, not main — run the main checkout's scripts/loopctl.py"
    for r in rels:  # absence claims added by this commit need a receipt (check-absence); older text is never re-checked
        if r.startswith("analysis/") and r.endswith(".md") and os.path.exists(os.path.join(REPO, r)):
            bad_n, rep = check_absence(os.path.join(REPO, r), "HEAD", REPO)
            if bad_n:
                return 2, "refused: " + "\n".join(rep[:6]) + "\n(run `re_check.py absent …` and cite its [receipt:<id>], or reword the claim)"
    if not claim("MAIN", pid):
        return 3, f"refused: MAIN is held (pid {holder(os.path.join(LOCKS, 'MAIN'))}) — retry after that landing"
    try:
        if git("diff", "--cached", "--quiet").returncode:
            return 2, "refused: the index already has staged changes (" + " ".join(git("diff", "--cached", "--name-only").stdout.split()) + ")"
        clean = [r for r in rels if not git("status", "--porcelain", "--", r).stdout.strip()]
        if clean:
            return 2, f"refused: nothing to commit in {', '.join(clean)}"
        if any(re.fullmatch(r"analysis/re-[^/]*\.md", r) for r in rels) and "analysis/INDEX.md" not in rels:
            subprocess.run([sys.executable, os.path.join(ROOT, "scripts/analysis_index.py"), "--root", REPO], capture_output=True)
            if git("status", "--porcelain", "--", "analysis/INDEX.md").stdout.strip():
                rels.append("analysis/INDEX.md")
        r = git("add", "--", *rels)
        if r.returncode:
            return 1, "git add failed: " + r.stderr.strip()
        body = msg.rstrip() + ("" if ATTRIBUTION in msg else f"\n\n{ATTRIBUTION}") + "\n"
        r = git("commit", "-q", "-m", body)
        if r.returncode:
            git("reset", "-q", "--", *rels)
            return 1, "git commit failed (unstaged again): " + (r.stderr or r.stdout).strip()
        return 0, "committed " + git("log", "--oneline", "-1").stdout.strip()
    finally:
        release("MAIN", pid)


REPORTS = os.path.join(LOOP, "reports")


def postmortem(job, since, lane_pid, lane):
    """After a session (autoloop.sh): if it left its claim or its card in-progress and wrote no report naming its claim
    or card since `since`, append a DIED line to the journal and free the claim. Returns the line ('' = it wrapped up)."""
    name, cid = job.get("claim"), job.get("card")
    if not name:
        return ""
    d = os.path.join(LOCKS, name)
    pid = holder(d) if os.path.isdir(d) else None
    left = pid is not None and (pid == str(lane_pid) or not held(pid))  # claims hold the LANE's pid, alive after the session
    c = cards().get(cid) if cid else None
    wip = bool(c) and c.get("status") == "in-progress"
    finish_attempt(job, since)
    keys = {k.lower() for k in (name, cid) if k}  # report names: <stamp>_<job>_<ID>.md, <stamp>_planner.md
    # whole name tokens: VTUR must not match VTUR2's report, nor AUDIT a `cleanse-audit` one
    reported = any(os.path.getmtime(f) >= since and keys & set(re.split(r"[_.]", os.path.basename(f).lower()))
                   for f in glob.glob(os.path.join(REPORTS, "*.md")))
    if reported or not (left or wip):
        return ""
    if left:
        release(name, pid)
    phase = (c or {}).get("phase") or job.get("phase") or "-"
    line = (f"- DIED {datetime.datetime.now():%Y-%m-%d %H:%M:%S} lane {lane}: job {job.get('job')} card {cid or '-'} "
            f"phase {phase}, claim {name} {'left (freed)' if left else 'released'}"
            + (", card still in-progress" if wip else "") + ", no report\n")
    open(JOURNAL, "a").write(line)
    return line


# ---------- start a build ----------
def start(cid):
    cs = cards()
    c = cs[cid]
    br = c.get("branch") or cid.lower()
    wt = os.path.join(WT_BASE, br)
    if not os.path.isdir(wt):
        r = subprocess.run(["git", "-C", ROOT, "worktree", "add", "-b", br, wt, "main"], capture_output=True, text=True)
        if r.returncode:
            r = subprocess.run(["git", "-C", ROOT, "worktree", "add", wt, br], capture_output=True, text=True)
            if r.returncode:
                sys.exit(r.stderr)
        for link in ("engine/assets_baked", "engine/artifacts", "extracted", "PC_swbf2_exe", "community_mods"):
            src = os.path.join(ROOT, link)
            if os.path.exists(src) and not os.path.lexists(os.path.join(wt, link)):
                os.symlink(src, os.path.join(wt, link))
    c["branch"] = br
    c["status"] = "in-progress"
    note_owner(c)
    write_card(c)
    write_backlog()
    target = os.path.join(ROOT, "engine/artifacts/lead_tmp", f"{br}_target")
    seed_target(target)
    print(f"worktree {wt}\nbranch {br}\nCARGO_TARGET_DIR={target}\n"
          f"gates: python3 {ROOT}/scripts/gates.py --tree {wt} --target {target} --out {ROOT}/engine/artifacts/lead_tmp/{br}/gates --auto")


# ---------- scratch cargo targets (TGTC) ----------
# Every lane builds into lead_tmp/<branch>_target, a reflink of engine/target (~45 GB apparent, ~20 GB exclusive once
# rebuilt). The tools delete them, not a session's ad-hoc sweep: land.py --commit removes the landed card's,
# `set status=parked|done|blocked` removes the card's own (and those nested in lead_tmp/<card>/), and `next` sweeps the rest in the background (dead_targets).
# A test run that overrides OPENBF_LOOP_DIR must also set OPENBF_LEAD_TMP, or no target is ever touched.
LEAD_TMP = os.environ.get("OPENBF_LEAD_TMP") or (None if os.environ.get("OPENBF_LOOP_DIR") else os.path.join(ROOT, "engine/artifacts/lead_tmp"))
TARGET_SEED = os.path.join(os.path.dirname(os.path.realpath(os.path.join(ROOT, "engine/artifacts"))), "target")  # main's, also from a worktree


def git_names(root=ROOT):
    """Every local branch name and every worktree basename."""
    b = subprocess.run(["git", "-C", root, "branch", "--format=%(refname:short)"], capture_output=True, text=True).stdout.split()
    w = subprocess.run(["git", "-C", root, "worktree", "list", "--porcelain"], capture_output=True, text=True).stdout
    return set(b) | {os.path.basename(l[9:]) for l in w.splitlines() if l.startswith("worktree ")}


def target_verdicts(lt, cs, live, names):
    """[(path, dead?, why)] for every lt/<name>_target. Pure: cards, live claim names and git names are passed in.
    Kept, always: a name (or its stem before the first -/_) of a live claim or an in-progress card. Dead: a *-scratch or
    *_pxc scratch target (`audit-<card>-*-scratch` too, once no AUDIT claim is live); a `<card>-render` target once its
    card is done/parked/blocked or missing; a card's own target once the card is done/parked/blocked, or while it is todo with no branch or
    worktree of that name (nothing to resume: `start` seeds a new one); a name with no card, branch or worktree."""
    protect = {k.lower() for k in live}
    by_name = {}
    for c in cs.values():
        ns = {c["id"].lower(), (c.get("branch") or "").lower()} - {""}
        for n in ns:
            by_name[n] = c
        if c.get("status") == "in-progress" or c["id"] in live:
            protect |= ns
    names = {n.lower() for n in names}
    out = []
    for d in sorted(glob.glob(os.path.join(lt, "*_target"))):
        if os.path.islink(d) or not os.path.isdir(d):
            continue
        name = os.path.basename(d)[:-len("_target")].lower()
        stem = re.split(r"[-_]", name)[0]
        c = by_name.get(name)
        if name in protect or stem in protect:
            out.append((d, False, "live claim / in-progress card"))
        elif name.endswith("-scratch") or name.endswith("_pxc"):
            out.append((d, True, "scratch target"))
        elif name.endswith("-render"):  # a capture's render target, `<card>-render` (PXWA/PXWB left 47 GB): its card's
            rc = next((x for x in cs.values() if x["id"].lower() == stem), None)
            if not rc or rc.get("status") in ("done", "parked", "blocked"):
                out.append((d, True, f"render target, card {stem.upper()} is {rc.get('status') if rc else 'missing'}"))
            else:
                out.append((d, False, f"render target, card {rc['id']} is {rc.get('status')}"))
        elif c and c.get("status") in ("done", "parked", "blocked"):
            out.append((d, True, f"card {c['id']} is {c['status']}"))
        elif c and c.get("status") == "todo" and name not in names:
            out.append((d, True, f"card {c['id']} is todo with no branch or worktree"))
        elif c:
            out.append((d, False, f"card {c['id']} is {c.get('status')}"))
        elif name in names:
            out.append((d, False, "live branch/worktree"))
        else:
            out.append((d, True, "no card, branch or worktree"))
    # A cargo target nested in a card's own scratch dir, lt/<card>/.../<dir>/CACHEDIR.TAG (card WEARY: HJAM's measuring
    # target lead_tmp/hjam/mtgt held 152 GB after the card was done): dead with its card, as the card's own target is.
    for d in nested_targets(lt):
        name = os.path.relpath(d, lt).split(os.sep)[0].lower()
        c = by_name.get(name)
        if c and c.get("status") in ("done", "parked", "blocked") and name not in protect and re.split(r"[-_]", name)[0] not in protect:
            out.append((d, True, f"nested target, card {c['id']} is {c['status']}"))
    return out


def nested_targets(lt, name=None):
    """Every cargo target dir (it holds a CACHEDIR.TAG) one to three levels inside lt/<name>/ (every <name> that is not a
    *_target when None); a target is not searched inside and a symlink is not followed."""
    out = []

    def walk(d, depth):
        try:
            subs = [e.path for e in os.scandir(d) if e.is_dir(follow_symlinks=False)]
        except OSError:
            return
        for sd in sorted(subs):
            if os.path.isfile(os.path.join(sd, "CACHEDIR.TAG")):
                out.append(sd)
            elif depth < 3:
                walk(sd, depth + 1)

    tops = [name] if name else [e.name for e in os.scandir(lt) if e.is_dir(follow_symlinks=False)] if os.path.isdir(lt) else []
    for n in sorted(tops):
        top = os.path.join(lt, n)
        if not n.endswith("_target") and os.path.isdir(top) and not os.path.islink(top):
            walk(top, 1)
    return out


def dead_targets():
    if not LEAD_TMP:
        return []
    return [(d, why) for d, dead, why in target_verdicts(LEAD_TMP, cards(), set(live_locks()), git_names(REPO)) if dead]


def target_busy(path):
    """A cargo build holding the target's lock, or a process running from / inside it."""
    import fcntl
    for prof in ("release", "debug"):
        lk = os.path.join(path, prof, ".cargo-lock")
        if os.path.exists(lk):
            try:
                with open(lk) as f:
                    fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                return True
            except OSError:
                pass
    return bool(procs_in(path))


def remove_target(path, quiet=False):
    """Delete one lead_tmp/<name>_target, or a cargo target (CACHEDIR.TAG) nested inside lead_tmp (never engine/target, a
    link, or anything outside lead_tmp). True if removed."""
    import shutil
    if not LEAD_TMP or not path:
        return False
    real, lt = os.path.realpath(path), os.path.realpath(LEAD_TMP)
    top = os.path.dirname(real) == lt and real.endswith("_target")
    nested = real.startswith(lt + os.sep) and os.path.isfile(os.path.join(real, "CACHEDIR.TAG"))
    if (os.path.islink(path) or not os.path.isdir(path) or not (top or nested)
            or real == os.path.realpath(TARGET_SEED)):
        return False
    if target_busy(path):
        quiet or print(f"busy, kept: {path}")
        return False
    shutil.rmtree(path, ignore_errors=True)
    gone = not os.path.exists(path)
    quiet or print(f"{'removed' if gone else 'FAILED to remove'}: {path}")
    return gone


def card_target(c):
    return os.path.join(LEAD_TMP, f"{c.get('branch') or c['id'].lower()}_target") if LEAD_TMP else None


def seed_target(path):
    """A missing branch target starts as a reflink of engine/target (~1 s on btrfs), so a build after a park costs ~10 s."""
    if not path or os.path.exists(path) or not os.path.isdir(TARGET_SEED) or target_busy(TARGET_SEED):
        return
    subprocess.run(["cp", "-a", "--reflink=always", TARGET_SEED, path], capture_output=True)


def sweep_targets(yes, quiet=False):
    """The `targets` command: list (dry run) or delete every dead target; one sweeper at a time. The dry run's rows
    (`path<TAB>why`) are what cleanse_delete.sh removes, so a target a build or a process is using is not among them."""
    if not LEAD_TMP:
        print("no lead_tmp (OPENBF_LOOP_DIR set without OPENBF_LEAD_TMP) — nothing to do"); return
    if yes and not claim("TARGETS", os.getpid()):
        quiet or print("another sweep holds TARGETS"); return
    try:
        for d, why in dead_targets():
            if yes:
                remove_target(d, quiet)
            elif target_busy(d):
                print(f"busy, kept: {d} ({why})")
            else:
                print(f"{d}\t{why}")
    finally:
        if yes:
            release("TARGETS", os.getpid())


# ---------- worktree sweep (card LOOPT; the maintainer's yes to STATUS question 49, INBOX 2026-10-15) ----------
def btrfs_excl(path):
    """Exclusive bytes of a path (btrfs filesystem du -s; symlinks are not followed), else None."""
    r = subprocess.run(["btrfs", "filesystem", "du", "-s", "--raw", path], capture_output=True, text=True)
    row = r.stdout.strip().splitlines()[-1].split() if r.returncode == 0 and r.stdout.strip() else []
    return int(row[1]) if len(row) >= 3 and row[1].isdigit() else None


def procs_in(path):
    """Pids of the processes whose cwd or executable is inside path, or with an argument naming a path inside it (a
    `cargo --target-dir <it>` or a sweep's `--tree <it>` started from elsewhere). Never this process."""
    reals = {p.rstrip("/") + "/" for p in (os.path.realpath(path), os.path.abspath(path))}
    inside = lambda s: any((s.rstrip("/") + "/").startswith(r) for r in reals)
    out = []
    for p in glob.glob("/proc/[0-9]*"):
        pid = int(os.path.basename(p))
        if pid == os.getpid():
            continue
        try:
            if any(inside(os.readlink(os.path.join(p, link))) for link in ("cwd", "exe") if os.path.lexists(os.path.join(p, link))):
                out.append(pid); continue
        except OSError:
            pass
        try:
            args = open(os.path.join(p, "cmdline"), "rb").read().decode(errors="replace").split("\0")
        except OSError:
            continue
        if any(inside(w) for a in args for w in re.split(r"[=\s]", a) if w.startswith("/")):
            out.append(pid)
    return out


def worktree_rows(repo=REPO, base=WT_BASE):
    """One dict per git worktree under base: path, name, head, branch (None = detached), on_main (HEAD is in main),
    dirty (porcelain lines), procs, created (the tree's .git file mtime)."""
    txt = subprocess.run(["git", "-C", repo, "worktree", "list", "--porcelain"], capture_output=True, text=True).stdout
    rows, cur = [], {}
    for line in txt.splitlines() + [""]:
        if not line.strip():
            if cur.get("path") and os.path.realpath(cur["path"]).startswith(os.path.realpath(base) + "/"):
                rows.append(cur)
            cur = {}
        elif line.startswith("worktree "):
            cur = {"path": line[9:], "branch": None, "head": ""}
        elif line.startswith("HEAD "):
            cur["head"] = line[5:]
        elif line.startswith("branch "):
            cur["branch"] = line[7:].removeprefix("refs/heads/")
    for r in rows:
        wt = r["path"]
        r["name"] = os.path.basename(wt)
        r["exists"] = os.path.isdir(wt)
        r["on_main"] = bool(r["head"]) and subprocess.run(["git", "-C", repo, "merge-base", "--is-ancestor", r["head"], "main"],
                                                           capture_output=True).returncode == 0
        st = subprocess.run(["git", "-C", wt, "status", "--porcelain"], capture_output=True, text=True) if r["exists"] else None
        r["dirty"] = st.stdout.splitlines() if st and st.returncode == 0 else []
        r["procs"] = procs_in(wt) if r["exists"] else []
        try:
            r["created"] = os.stat(os.path.join(wt, ".git")).st_mtime
        except OSError:
            r["created"] = 0
    return rows


def worktree_card(name):
    """(card id, owner claim) a worktree belongs to by its name: `audit-<card>-...` is an audit's (claim AUDIT), else
    the stem before the first -/_ is the card (and its claim)."""
    audit = name.startswith("audit-")
    stem = re.split(r"[-_]", name[6:] if audit else name)[0].upper()
    return stem, ("AUDIT" if audit else stem)


def worktree_mentions(rows, cs):
    """{tree name: an open card whose text names it} - a card's resume may point into a scratch tree."""
    texts = {c["id"]: open(c["_path"], errors="replace").read() for c in cs.values() if c.get("status") != "done" and c.get("_path")}
    out = {}
    for r in rows:
        rx = re.compile(r"(?<![\w-])" + re.escape(r["name"]) + r"(?![\w-])")
        hit = next((cid for cid, t in texts.items() if rx.search(t)), None)
        if hit:
            out[r["name"]] = hit
    return out


def worktree_verdicts(rows, cs, live, claim_since, mentions=None):
    """[(row, candidate?, why)]. Pure: cards, live claim names and each live claim's start time are passed in.
    Candidate: a detached *-scratch or <card>-render tree whose HEAD is on main, or a branch tree whose card is done and whose branch is
    merged into main. Kept: commits not on main, a live claim (an audit's tree only when made since its claim began),
    a card not done, a process running inside it, an open card that names it."""
    out, mentions = [], mentions or {}
    for r in rows:
        cid, owner = worktree_card(r["name"])
        c = cs.get(cid)
        status = c.get("status") if c else None
        if not r.get("exists", True):
            out.append((r, True, "directory gone (prune only)")); continue
        if r["procs"]:
            why = f"a process runs in it (pid {', '.join(map(str, r['procs'][:3]))})"
        elif not r["on_main"]:
            why = "commits not on main"
        elif r["name"] in mentions:
            why = f"open card {mentions[r['name']]} names it"
        elif owner in live and (owner != "AUDIT" or r["created"] >= claim_since.get(owner, 0) - 60):
            why = f"claim {owner} is live"
        elif r["branch"] is None and not r["name"].endswith(("-scratch", "-render")):
            why = "detached but not a *-scratch / *-render tree"
        elif r["branch"] is None and owner != "AUDIT" and c and status != "done":
            why = f"card {cid} is {status}"
        elif r["branch"] is not None and not (c and status == "done"):
            why = f"branch tree, card {cid} is {status or 'missing'}"
        else:
            out.append((r, True, f"{'scratch' if r['branch'] is None else 'merged branch'}, card {cid} {status or '(none)'}")); continue
        out.append((r, False, why))
    return out


def sweep_worktrees(yes):
    """The `worktrees` command: list every openbf-wt tree with its verdict; --yes saves each candidate's diff and untracked
    list under lead_tmp/worktree_sweep/<date>/, removes it (rm -r + git worktree prune) and its <name>_target, and prints
    the space freed."""
    import shutil
    if yes and not claim("WORKTREES", os.getpid()):
        print("another sweep holds WORKTREES"); return 1
    try:
        live = set(live_locks())
        since = {}
        for n in live:
            try:
                since[n] = os.stat(os.path.join(LOCKS, n)).st_mtime
            except OSError:
                pass
        rows, cs = worktree_rows(), cards()
        verdicts = worktree_verdicts(rows, cs, live, since, worktree_mentions(rows, cs))
        gb = lambda b: f"{(b or 0) / 2 ** 30:.1f} GB"
        save_dir = os.path.join(LEAD_TMP or os.path.join(LOOP, "lead_tmp"), "worktree_sweep", f"{datetime.date.today()}")
        free0 = shutil.disk_usage(WT_BASE).free if os.path.isdir(WT_BASE) else 0
        freed, removed = 0, []
        for r, cand, why in verdicts:
            tgt = os.path.join(LEAD_TMP, f"{r['name']}_target") if LEAD_TMP else None
            size = btrfs_excl(r["path"]) if r.get("exists", True) else 0
            tsize = btrfs_excl(tgt) if tgt and os.path.isdir(tgt) and not os.path.islink(tgt) else 0
            cid, _ = worktree_card(r["name"])
            print(f"{'CANDIDATE' if cand else 'kept     '} {r['name']:28s} {r['head'][:8]} {r['branch'] or '(detached)':10s} "
                  f"card {cid}:{(cards().get(cid) or {}).get('status', '-')} dirty {len(r['dirty'])} "
                  f"tree {gb(size)} target {gb(tsize)} - {why}")
            if not (yes and cand):
                continue
            real = os.path.realpath(r["path"])
            if not real.startswith(os.path.realpath(WT_BASE) + "/") or real == os.path.realpath(REPO):
                print(f"  refused: {real} is not under {WT_BASE}"); continue
            if r.get("exists", True):
                os.makedirs(save_dir, exist_ok=True)
                d = subprocess.run(["git", "-C", real, "diff", "HEAD"], capture_output=True, text=True)
                open(os.path.join(save_dir, f"{r['name']}.patch"), "w").write(f"# {r['name']} HEAD {r['head']}\n" + d.stdout)
                un = subprocess.run(["git", "-C", real, "ls-files", "--others", "--exclude-standard", "-z"],
                                    capture_output=True, text=True).stdout.split("\0")
                un = [f for f in un if f and not os.path.islink(os.path.join(real, f))]
                L = []
                for f in un:  # a scratch tree's new probe files are its work: copy them (up to 20 MB each), list the rest
                    src, n = os.path.join(real, f), 0
                    try:
                        n = os.path.getsize(src)
                        if n <= 20 << 20:
                            dst = os.path.join(save_dir, f"{r['name']}.files", f)
                            os.makedirs(os.path.dirname(dst), exist_ok=True); shutil.copy2(src, dst)
                    except OSError:
                        pass
                    L.append(f"{n}\t{'saved' if n <= 20 << 20 else 'NOT saved (over 20 MB)'}\t{f}\n")
                open(os.path.join(save_dir, f"{r['name']}.untracked"), "w").write("".join(L))
                shutil.rmtree(real, ignore_errors=True)
            gone = not os.path.exists(real)
            if gone:
                removed.append(r["name"]); freed += (size or 0)
                if tgt and remove_target(tgt, quiet=True):
                    freed += tsize or 0
            print(f"  {'removed' if gone else 'FAILED to remove'} {real}" + (f" (diff + untracked files in {save_dir})" if gone else ""))
        if yes:
            subprocess.run(["git", "-C", REPO, "worktree", "prune"], capture_output=True)
            free1 = shutil.disk_usage(WT_BASE).free if os.path.isdir(WT_BASE) else 0
            print(f"removed {len(removed)} tree(s): {' '.join(removed) or '-'}; exclusive size before removal {gb(freed)}; "
                  f"free space {gb(free0)} -> {gb(free1)} (+{gb(free1 - free0)}; btrfs frees in the background)")
        else:
            n = sum(1 for _, c_, _ in verdicts if c_)
            print(f"{n} candidate(s) of {len(verdicts)} trees (dry run; `worktrees --yes` removes them)")
        return 0
    finally:
        if yes:
            release("WORKTREES", os.getpid())


def lead_tmp_line():
    if not LEAD_TMP or not os.path.isdir(LEAD_TMP):
        return "lead_tmp: (none)"
    n = len([d for d in glob.glob(os.path.join(LEAD_TMP, "*_target")) if os.path.isdir(d)])
    gb = lambda b: f"{int(b) / 2 ** 30:.0f} GB"
    r = subprocess.run(["btrfs", "filesystem", "du", "-s", "--raw", LEAD_TMP], capture_output=True, text=True)
    row = r.stdout.strip().splitlines()[-1].split() if r.returncode == 0 and r.stdout.strip() else []
    if len(row) >= 3 and row[0].isdigit():
        return f"lead_tmp: {gb(row[0])} apparent, {gb(row[1])} exclusive (btrfs filesystem du -s), {n} *_target dirs"
    ap = subprocess.run(["du", "-sb", LEAD_TMP], capture_output=True, text=True).stdout.split()
    return f"lead_tmp: {gb(ap[0]) if ap else '?'} apparent (btrfs filesystem du failed: exclusive unknown), {n} *_target dirs"


# ---------- scoreboard ----------
def approvals_lines(cs=None):
    """The live [approvals] of gates.toml as lines for STATUS: what each lets through, the bug card an EXPOSED one owes and
    how long it has stood. The list is a debt: an exposed bug nobody fixes is a regression that was waved through."""
    import tomllib
    cs = cs or cards()
    try:
        ap = tomllib.load(open(os.path.join(LOOP, "gates.toml"), "rb")).get("approvals", {})
    except (OSError, ValueError) as e:
        return [f"(gates.toml unreadable: {e})"]
    if not ap:
        return []
    L = ["", "### Approvals in force (gates.toml [approvals]) - a debt list", ""]
    for cid, a in sorted(ap.items()):
        c = cs.get(cid, {})
        what = "; ".join(f"{k} = {a[k]}" for k in ("fidelity", "oracle") if a.get(k))
        age = ""
        if a.get("since"):
            try:
                age = f", {(datetime.date.today() - datetime.date.fromisoformat(str(a['since']))).days} d old"
            except ValueError:
                pass
        line = f"- **{cid}** ({c.get('status', 'no such card')}{age}): {what}"
        ex = [x for x in re.split(r"[,\s]+", str(a.get("exposes", ""))) if x]
        if ex:
            owed = ", ".join(f"{x} {cs.get(x, {}).get('status', '?')}" for x in ex)
            line += f" - EXPOSED, owes: {owed}"
            if all(cs.get(x, {}).get("status") == "done" for x in ex):
                line += " -> every owed card is done: REMOVE this approval and re-take the fidelity baseline"
        if c.get("status") == "done" and not ex:
            line += " -> the card is done: REMOVE this approval"
        L.append(line)
    return L


def scoreboard():
    import tomllib
    cs = cards()
    L = [f"## Scoreboard ({datetime.datetime.now():%Y-%m-%d %H:%M}, `loopctl.py scoreboard`)", ""]
    ms = sorted({c.get("milestone", "?") for c in cs.values()})
    L += ["| milestone | open | in progress | parked | blocked | done |", "|---|---|---|---|---|---|"]
    for m in ms:
        cnt = {k: sum(1 for c in cs.values() if c.get("milestone") == m and c.get("status") == k) for k in STATUSES}
        L.append(f"| {m} | {cnt['todo']} | {cnt['in-progress']} | {cnt['parked']} | {cnt['blocked']} | {cnt['done']} |")
    j = open(JOURNAL).read() if os.path.exists(JOURNAL) else ""
    now = datetime.datetime.now()
    ends = re.findall(r"^- ended (\d{4}-\d\d-\d\d \d\d:\d\d:\d\d) \(lane \d+\):.*?turns=(\d+|None|) est_cost=\$([0-9.]+)", j, re.M)
    def window(h):
        xs = [(t, float(c)) for t, _, c in ends if now - datetime.datetime.strptime(t, "%Y-%m-%d %H:%M:%S") < datetime.timedelta(hours=h)]
        return len(xs), sum(c for _, c in xs)
    landed = re.findall(r"^- LANDED (\S+)", j, re.M)
    n24, c24 = window(24); n7, c7 = window(24 * 7)
    L += ["", f"Sessions / est. spend: last 24 h {n24} / ${c24:.0f}; last 7 d {n7} / ${c7:.0f}. Landings recorded in the journal: {len(landed)} total."]
    try:
        g = tomllib.load(open(os.path.join(ROOT, "engine/artifacts/loop/gates.toml"), "rb"))
        L.append(f"Tests on main: {g['ci']['passed']} passed / {g['ci']['failed']} failed / {g['ci']['ignored']} ignored. "
                 f"Round sweep: `{g['sweep']['footer']}`.")
    except Exception as e:
        L.append(f"(gates.toml unreadable: {e})")
    al = [c for c in cs.values() if is_alpha(c)]
    ac = {k: sum(1 for c in al if c.get("status") in ks) for k, ks in
          (("open", ("todo",)), ("progress", ("in-progress", "parked")), ("done", ("done",)))}
    blk = [c["id"] for c in al if c.get("status") != "done" and (c.get("status") == "blocked" or (c.get("needs") or "").strip()
                                                                  or needs_acceptance(c))]
    L.append(f"M-ALPHA: {len(al)} alpha cards — {ac['open']} open / {ac['progress']} in progress / {ac['done']} done; "
             f"blockers: {', '.join(blk) or 'none'}. Open cards tagged `defer: post-alpha`: "
             f"{sum(1 for c in cs.values() if is_deferred(c) and c.get('status') != 'done')}; `acceptance: needed`: "
             f"{sum(1 for c in cs.values() if needs_acceptance(c) and c.get('status') != 'done')}.")
    if os.path.exists(AUDITS):
        a = audit_log(open(AUDITS).read())
        w = sum(1 for _, v, _ in a if v != "OK")
        L.append(f"Auditor: {len(a)} audits, {w} found a problem" + (f" ({100 * w / len(a):.0f} %)" if a else ""))
        per = {}
        for _, v, d in a:
            per.setdefault(d, [0, 0])[0] += v != "OK"; per[d][1] += 1
        L.append("Audits by domain (found a problem / audited): "
                 + ", ".join(f"{d} {x}/{n} ({100 * x / n:.0f} %)" for d, (x, n) in sorted(per.items(), key=lambda kv: -kv[1][1])))
    try:  # card PARITY: how close main is to the PS2 (loop/parity/, `parity.py main` refreshes it)
        import parity
        L.append(parity.scoreboard_line())
    except Exception as e:
        L.append(f"(parity unreadable: {e})")
    st = state()
    L.append(lead_tmp_line())
    L.append(f"Counters: landings since planner {st.get('landings_since_planner', 0)}, since cleanse {st.get('landings_since_cleanse', 0)}, "
             f"sessions since audit {st.get('sessions_since_audit', 0)}.")
    L += approvals_lines(cs)
    return "\n".join(L)


# ---------- absence-claim receipts ----------
RECEIPTS = os.path.join(LOOP, "receipts")
ABSENCE_RES = [re.compile(x, re.I) for x in (
    r"\bno class(es)?\b", r"\bnone of the class", r"\bno (class|file|chunk|record|script|function|code|caller|writer|reader|author)s?\b[^.\n]{0,30}\b(author|write|writes|set|sets|read|reads|call|calls|use|uses|contain|contains|reference|references)",
    r"\bnone (author|write|set|call|use|contain)", r"\babsent\b", r"\bnever (authored|authors|written|written to|set|called|read|used|referenced)\b",
    r"\bnot present\b", r"\bnothing (authors|writes|sets|calls|reads|uses|references)\b", r"\bno (callers?|writers?|hits?|users?)\b",
    r"\bdoes not (exist|appear) (anywhere|in any)\b")]
RECEIPT_TAG = re.compile(r"\[receipt:([0-9a-f]{6,16})\]")


def receipt_exists(rid):
    for d in {RECEIPTS, os.path.join(ROOT, "engine/artifacts/loop/receipts")}:
        if glob.glob(os.path.join(d, f"*{rid}.json")):
            return True
    return False


def absence_claims(text, only=None):
    """[(line no, text, receipt ids, missing ids)] for each absence-claim line outside code fences. A receipt tag counts on
    the claim's line or the next 3 lines of its paragraph. `only` = set of line numbers to look at (None = all)."""
    L, out, fence = text.split("\n"), [], False
    for i, ln in enumerate(L):
        if ln.lstrip().startswith("```"):
            fence = not fence
            continue
        if fence or (only is not None and i + 1 not in only) or not any(r.search(ln) for r in ABSENCE_RES):
            continue
        ids = []
        for j in range(i, min(i + 4, len(L))):
            if j > i and not L[j].strip():
                break
            ids += RECEIPT_TAG.findall(L[j])
        out.append((i + 1, ln.strip(), ids, [r for r in ids if not receipt_exists(r)]))
    return out


def added_lines(doc, base, repo=None):
    """Line numbers of `doc` added since `base` (all of them when the file is untracked at base)."""
    repo = repo or REPO
    rel = os.path.relpath(os.path.abspath(doc), repo)
    if git("cat-file", "-e", f"{base}:{rel}", repo=repo).returncode:
        return None
    nums = set()
    for h in re.finditer(r"^@@ -\S+ \+(\d+)(?:,(\d+))? @@", git("diff", "-U0", base, "--", rel, repo=repo).stdout, re.M):
        a, n = int(h.group(1)), int(h.group(2) or 1)
        nums.update(range(a, a + n))
    return nums


def check_absence(doc, base=None, repo=None):
    """Returns (failures, report lines). A failure = an absence claim with no receipt tag, or a tag naming no receipt."""
    text = open(doc, errors="replace").read()
    only = added_lines(doc, base, repo) if base else None
    bad, rep = 0, []
    for n, t, ids, missing in absence_claims(text, only):
        if not ids:
            bad += 1; rep.append(f"{doc}:{n}: absence claim without a [receipt:<id>] — run `re_check.py absent …` and cite it: {t[:110]}")
        elif missing:
            bad += 1; rep.append(f"{doc}:{n}: receipt {', '.join(missing)} does not exist in {os.path.relpath(RECEIPTS, ROOT)}/")
    return bad, rep


# ---------- metrics ----------
def run_sessions():
    """[dict(day, lane, cost, turns, err)] from runs/run_<date>_<time>_<lane|manual_X>.json (the claude -p results); probes skipped."""
    out = []
    for f in sorted(glob.glob(os.path.join(LOOP, "runs", "run_*.json"))):
        m = re.match(r"run_(\d{4})(\d\d)(\d\d)_\d+(?:_(.+))?\.json$", os.path.basename(f))
        if not m:
            continue
        try:
            r = json.load(open(f))
        except (ValueError, OSError):
            continue
        if not isinstance(r, dict) or "total_cost_usd" not in r:
            continue
        out.append(dict(day=f"{m.group(1)}-{m.group(2)}-{m.group(3)}", lane=m.group(4) or "lane?", cost=float(r["total_cost_usd"] or 0),
                        turns=int(r.get("num_turns") or 0), err=bool(r.get("is_error"))))
    return out


def followup_ids(cs):
    """Cards that are follow-ups: id = another card's id + one trailing digit/letter (ANNC2 of ANNC), or the body says so."""
    out = set()
    for cid, c in cs.items():
        if len(cid) > 2 and cid[:-1] in cs and re.fullmatch(r"[2-9A-Z]", cid[-1]) and cid[:-1] != cid:
            out.add(cid)
        elif re.search(r"follow-?up|found by|spun off|split from", c.get("_body", "")[:600], re.I):
            out.add(cid)
    return out


def metrics():
    """Returns (rows [(section, key, value)], summary lines)."""
    cs = cards()
    j = open(JOURNAL).read() if os.path.exists(JOURNAL) else ""
    R, S = [], []
    land = {}
    for c in cs.values():  # a landed card's `resume:` reads `landed <sha> <date>`
        m = re.match(r"landed \S+ (\d{4}-\d\d-\d\d)", c.get("resume", ""))
        if m:
            land[m.group(1)] = land.get(m.group(1), 0) + 1
    eng = {}
    for d in git("log", "--format=%ad", "--date=short", "--", "engine/").stdout.split():
        eng[d] = eng.get(d, 0) + 1
    for d in sorted(set(land) | set(eng)):
        R += [("landings_per_day", d, land.get(d, 0)), ("engine_commits_per_day", d, eng.get(d, 0))]
    S.append(f"Landings (cards' `landed` stamps): {sum(land.values())} over {len(land)} days, busiest {max(land.items(), key=lambda kv: kv[1]) if land else '-'}; "
             f"engine/ commits: {sum(eng.values())}.")
    ms = sorted({c.get("milestone", "?") for c in cs.values()})
    for m in ms:
        for st_ in STATUSES:
            R.append(("cards_by_milestone", f"{m}/{st_}", sum(1 for c in cs.values() if c.get("milestone") == m and c.get("status") == st_)))
    done = sum(1 for c in cs.values() if c.get("status") == "done")
    S.append(f"Cards: {len(cs)} total, {done} done; done by milestone: "
             + ", ".join(f"{m} {sum(1 for c in cs.values() if c.get('milestone') == m and c.get('status') == 'done')}" for m in ms))
    fu = followup_ids(cs)
    R += [("followups", "follow_up_cards", len(fu)), ("followups", "cards", len(cs)),
          ("followups", "ratio", f"{len(fu) / max(1, len(cs)):.3f}")]
    S.append(f"Follow-up ratio: {len(fu)}/{len(cs)} cards = {100 * len(fu) / max(1, len(cs)):.0f} % (id = parent+digit/letter, or body says follow-up/found by).")
    verd = {}
    if os.path.exists(AUDITS):
        for _, v, d in audit_log(open(AUDITS).read()):
            verd[(d, v)] = verd.get((d, v), 0) + 1
    for (d, v), n in sorted(verd.items()):
        R.append(("audit_verdicts", f"{d}/{v}", n))
    rep = {}
    for f in glob.glob(os.path.join(REPORTS, "*audit*.md")):
        m = re.search(r"^# .*— (OK|PARTLY|WRONG)\b", open(f, errors="replace").readline())
        if m:
            rep[m.group(1)] = rep.get(m.group(1), 0) + 1
    for v, n in sorted(rep.items()):
        R.append(("audit_report_verdicts", v, n))
    tot = {}
    for (_, v), n in verd.items():
        tot[v] = tot.get(v, 0) + n
    S.append(f"Audit verdicts (audits.md): {tot or 'none'}; audit report headings: {rep or 'none'}.")
    ses = run_sessions()
    bylane = {}
    for x in ses:
        v = bylane.setdefault(x["lane"].split("_")[0] if x["lane"].startswith("lane") else "manual", [0, 0.0, 0, 0])
        v[0] += 1; v[1] += x["cost"]; v[2] += x["turns"]; v[3] += x["err"]
    for k, (n, c, t, e) in sorted(bylane.items()):
        R += [("session_cost", f"{k}/sessions", n), ("session_cost", f"{k}/total_usd", f"{c:.2f}"),
              ("session_cost", f"{k}/mean_usd", f"{c / n:.2f}"), ("session_cost", f"{k}/mean_turns", f"{t / n:.0f}"), ("session_cost", f"{k}/errors", e)]
    perday = {}
    for x in ses:
        v = perday.setdefault(x["day"], [0, 0.0]); v[0] += 1; v[1] += x["cost"]
    for d, (n, c) in sorted(perday.items()):
        R += [("sessions_per_day", d, n), ("cost_per_day", d, f"{c:.2f}")]
    tc, nl = sum(x["cost"] for x in ses), sum(land.values())
    S.append(f"Sessions with logged cost (runs/): {len(ses)}, ${tc:.0f} total, ${tc / max(1, len(ses)):.1f} mean, ${tc / max(1, nl):.0f} per landed card; "
             + ", ".join(f"{k} {n}x${c / n:.0f}" for k, (n, c, _, _) in sorted(bylane.items())))
    wfn = len(re.findall(r"wf=1\b", j))
    R.append(("workflows", "journal_sessions_wf1", wfn))
    try:
        import tomllib
        g = tomllib.load(open(os.path.join(LOOP, "gates.toml"), "rb"))["ci"]
        R += [("tests", "passed", g["passed"]), ("tests", "failed", g["failed"]), ("tests", "ignored", g["ignored"])]
        S.append(f"Tests on main (gates.toml): {g['passed']} passed / {g['failed']} failed / {g['ignored']} ignored.")
    except Exception as e:
        S.append(f"(tests: gates.toml unreadable: {e})")
    return R, S


# ---------- cli ----------
def self_test():
    """pick() over fixture card sets in a scratch loop dir (OPENBF_LOOP_DIR / OPENBF_CONTROL_DIR); returns 0 on pass."""
    import tempfile
    scenarios = [  # (cards as (id, value, phase), [(lane, expected card)] taken in order, each claim held by this process)
        ("two lanes, value 5 before value 4",
         [("RE4", 4, "re"), ("BLD4", 4, "build"), ("CAP5", 5, "capture"), ("BLD5", 5, "build"), ("MEA4", 4, "measure")],
         [(1, "BLD5"), (2, "CAP5"), (2, "RE4"), (1, "BLD4")]),
        ("a lone value-5 capture card goes first on either lane",
         [("BLD4", 4, "build"), ("CLN4", 4, "cleanup"), ("CAP5", 5, "capture"), ("RE4", 4, "re"), ("MEA4", 4, "measure")],
         [(1, "CAP5"), (1, "BLD4")]),
        ("lane 2 takes it too",
         [("BLD4", 4, "build"), ("RE4", 4, "re"), ("CAP5", 5, "capture"), ("CLN4", 4, "cleanup"), ("MEA4", 4, "measure")],
         [(2, "CAP5"), (2, "RE4")]),
        # M-ALPHA tags (LRUL): a 4th field is extra header lines
        ("at equal value alpha goes first, over lane 2's phase preference too",
         [("UNT5", 5, "build"), ("ALP5", 5, "build", "alpha: yes"), ("UNT4", 4, "re"), ("ALP4", 4, "build", "alpha: yes"),
          ("UNT1", 1, "measure")],
         [(1, "ALP5"), (1, "UNT5"), (2, "ALP4"), (2, "UNT4")]),
        ("a deferred card loses to untagged and alpha ones of lower value; taken when nothing else is eligible",
         [("DEF5", 5, "build", "defer: post-alpha"), ("DEF4", 4, "re", "defer: post-alpha"), ("UNT3", 3, "build"),
          ("ALP2", 2, "build", "alpha: yes"), ("DEF1", 1, "capture", "defer: post-alpha")],
         [(1, "UNT3"), (1, "ALP2"), (1, "DEF5"), (2, "DEF4")]),
        ("an acceptance: needed card is never picked (the planner refill instead)",
         [("ACC5", 5, "build", "acceptance: needed"), ("ACA5", 5, "re", "alpha: yes\nacceptance: needed"),
          ("ACD4", 4, "build", "defer: post-alpha\nacceptance: needed"), ("ACC3", 3, "measure", "acceptance: needed"),
          ("UNT2", 2, "re")],
         [(1, "UNT2"), (1, None)]),
        ("the maintainer's `first:` cards go before any value, lowest number first, on both lanes",
         [("ALP5", 5, "build", "alpha: yes"), ("RE5", 5, "re"), ("FST3", 3, "build", "first: 2"), ("FST4", 4, "re", "first: 1"),
          ("UNT1", 1, "measure")],
         [(1, "FST4"), (2, "FST3"), (2, "ALP5"), (1, "RE5")]),
    ]
    bad = 0
    for name, fixture, steps in scenarios:
        with tempfile.TemporaryDirectory(prefix="loopctl_selftest_") as d:
            os.makedirs(os.path.join(d, "cards"))
            for cid, v, ph, *extra in fixture:
                open(os.path.join(d, "cards", f"{cid}.md"), "w").write(
                    f"---\nid: {cid}\ntitle: t\nmilestone: M1\nvalue: {v}\nphase: {ph}\nstatus: todo\n"
                    + "".join(f"{x}\n" for x in extra) + "---\n")
            open(os.path.join(d, "STATUS.md"), "w").write("fixture\n")  # no STATUS -> planner
            env = dict(os.environ, OPENBF_LOOP_DIR=d, OPENBF_CONTROL_DIR=d, OPENBF_LEAD_TMP="")  # "" = no target sweep
            for lane, want in steps:
                out = subprocess.run([sys.executable, os.path.abspath(__file__), "next", "--lane", str(lane), "--lanes", "2",
                                      "--pid", str(os.getpid())], env=env, capture_output=True, text=True)
                try:
                    got = json.loads(out.stdout).get("card")
                except Exception:
                    got = f"<{out.stdout.strip()} {out.stderr.strip()}>"
                ok = got == want
                bad += not ok
                print(f"{'ok  ' if ok else 'FAIL'} {name}: lane {lane} -> {got} (want {want})")
    bad += self_test_targets()
    bad += self_test_claims()
    bad += self_test_audits()
    bad += self_test_audit_holds()
    bad += self_test_commit_docs()
    bad += self_test_postmortem()
    bad += self_test_lane3()
    bad += self_test_effort()
    bad += self_test_pick_guards()
    bad += self_test_worktrees()
    bad += self_test_files()
    print("self-test: " + ("PASS" if not bad else f"{bad} FAILED"))
    return 1 if bad else 0


def self_test_pick_guards():
    """Card LRES: pick() leaves alone the card of a live CLEANSE session, a branch a live land.py is on and a card that
    waits on the maintainer; a parked card is a landing only with a branch that exists, else a card once its deps are done."""
    import tempfile
    bad = 0
    me, dead = str(os.getpid()), "999999999"

    def run(d, args, extra_env=None):
        env = dict(os.environ, OPENBF_LOOP_DIR=d, OPENBF_CONTROL_DIR=d, OPENBF_LEAD_TMP=os.path.join(d, "lt"),
                   OPENBF_REPO=os.path.join(d, "repo"), **{"OPENBF_JOB_JSON": "", **(extra_env or {})})
        return subprocess.run([sys.executable, os.path.abspath(__file__)] + args, env=env, capture_output=True, text=True)

    def card(d, cid, v, status, extra=""):
        open(os.path.join(d, "cards", f"{cid}.md"), "w").write(
            f"---\nid: {cid}\ntitle: t\nmilestone: M0\nvalue: {v}\nphase: build\nstatus: {status}\n{extra}---\n")

    def lock(d, name, pid):
        os.makedirs(os.path.join(d, "locks", name), exist_ok=True)
        open(os.path.join(d, "locks", name, "pid"), "w").write(pid)

    def fixture(d):
        os.makedirs(os.path.join(d, "cards")); os.makedirs(os.path.join(d, "lt"))
        repo = os.path.join(d, "repo")
        for cmd in (["init", "-q", repo], ["-C", repo, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "x"],
                    ["-C", repo, "branch", "built"]):
            subprocess.run(["git"] + cmd, check=True, capture_output=True)
        open(os.path.join(d, "STATUS.md"), "w").write("fixture\n")
        for i in range(5):  # fewer than 5 open cards would make the planner the job
            card(d, f"FIL{i}", 1, "todo")

    def check(name, d, want, lanes_pid=me):
        nonlocal bad
        out = run(d, ["next", "--lane", "1", "--lanes", "1", "--pid", lanes_pid])
        try:
            j = json.loads(out.stdout)
            got = (j.get("job"), j.get("card"))
        except Exception:
            got = f"<{out.stdout.strip()} {out.stderr.strip()}>"
        ok = got == want
        bad += not ok
        print(f"{'ok  ' if ok else 'FAIL'} pick guards: {name}: {got} (want {want})")
        if isinstance(got, tuple) and got[1]:  # free the claim so the next check starts clean
            run(d, ["release", got[1]])

    # (1) a cleanse's card: opened through the real `new` + `set` under the session's env
    with tempfile.TemporaryDirectory(prefix="loopctl_selftest_") as d:
        fixture(d)
        sess = {"OPENBF_JOB_JSON": json.dumps({"job": "cleanse", "claim": "CLEANSE"}), "OPENBF_LANE_PID": me}
        lock(d, "CLEANSE", me)
        run(d, ["new", "CLNX", "--title", "t", "--milestone", "M0", "--value", "5", "--phase", "cleanup"], sess)
        run(d, ["set", "CLNX", "status=in-progress"], sess)
        check("a live cleanse's in-progress CLN card is not resumed", d, ("card", "FIL0"))
        lock(d, "CLEANSE", dead)
        check("the cleanse holder dead: it is resumed", d, ("resume", "CLNX"))
    # (1b) a live land.py on the card's branch (a session outside the launcher: no owner line)
    with tempfile.TemporaryDirectory(prefix="loopctl_selftest_") as d:
        fixture(d)
        card(d, "LND5", 5, "in-progress", "branch: built\n")
        os.makedirs(os.path.join(d, "lt", "built", "land"))
        p = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)", "land.py"])
        try:
            json.dump({"running": True, "pid": p.pid}, open(os.path.join(d, "lt", "built", "land", "state.json"), "w"))
            check("an in-progress card with a live land.py on its branch is not resumed", d, ("card", "FIL0"))
        finally:
            p.kill(); p.wait()
        check("its land.py dead: it is resumed", d, ("resume", "LND5"))
    # (2) parked cards
    with tempfile.TemporaryDirectory(prefix="loopctl_selftest_") as d:
        fixture(d)
        card(d, "DEP5", 5, "blocked")
        card(d, "PRK5", 5, "parked", "deps: DEP5\nresume: write the check first\n")
        check("a parked card with no branch and an unmet dep is not returned", d, ("card", "FIL0"))
        card(d, "DEP5", 5, "done")
        check("its dep done: returned as a card, not a landing", d, ("card", "PRK5"))
        card(d, "PRK5", 5, "parked", "branch: gone\n")
        check("a parked card whose branch does not exist is a card too", d, ("card", "PRK5"))
        card(d, "PRK5", 5, "parked", "branch: built\n")
        check("a parked card with a branch is a landing", d, ("land", "PRK5"))
        card(d, "PRK5", 5, "parked", "branch: built\nneeds: maintainer - the adapter is gone\n")
        check("a parked card with a branch and needs: is not a landing", d, ("card", "FIL0"))
    # PKHD: waiting parked work releases files; clearing needs restores its reservation.
    with tempfile.TemporaryDirectory(prefix="loopctl_selftest_") as d:
        fixture(d)
        card(d, "PRK5", 5, "parked", "branch: built\nfiles: shared.rs\nneeds: maintainer - a question\n")
        card(d, "USE5", 5, "todo", "files: shared.rs\n")
        check("parked + needs releases its files", d, ("card", "USE5"))
        card(d, "PRK5", 5, "parked", "branch: built\nfiles: shared.rs\nneeds: \n")
        # Keep the landing claimed so pick reaches eligibility without taking PRK5 itself.
        lock(d, "MAIN", me)
        check("parked + needs cleared reserves its files", d, ("card", "FIL0"))
    # (3) waiting on the maintainer
    with tempfile.TemporaryDirectory(prefix="loopctl_selftest_") as d:
        fixture(d)
        card(d, "NDS5", 5, "in-progress", "needs: maintainer - a question\n")
        check("an in-progress card with needs: is not resumed", d, ("card", "FIL0"))
        card(d, "NDS5", 5, "in-progress")
        check("the same card without needs: is", d, ("resume", "NDS5"))
    # (4) a card blocked on the fidelity gate calls the planner after 3 sessions - not while it waits on the maintainer (CLNV)
    with tempfile.TemporaryDirectory(prefix="loopctl_selftest_") as d:
        fixture(d)
        three = {"landings_since_planner": 0, "landings_since_cleanse": 0, "sessions_since_planner": 3, "sessions_since_audit": 0,
                 "last_job": "card", "last_landed_milestone": {}}
        json.dump(three, open(os.path.join(d, "state.json"), "w"))
        card(d, "FID5", 5, "blocked", "resume: fidelity WORSE undeclared - idle20@kam1\n")
        check("a card blocked on a fidelity refusal: the planner after 3 sessions", d, ("planner", None))
        run(d, ["release", "PLANNER"])
        json.dump(three, open(os.path.join(d, "state.json"), "w"))
        card(d, "FID5", 5, "blocked", "resume: fidelity WORSE undeclared - idle20@kam1\nneeds: maintainer - may it land?\n")
        check("the same card waiting on the maintainer: no planner for it", d, ("card", "FIL0"))
    # RETRYCAP: every selection uses the public next CLI with fixture controls and no real claim.
    for label, phase, count, override, status, branch, want in (
            ("build at cap", "build", 3, "", "todo", "", ("card", "FIL0")),
            ("build below cap", "build", 2, "", "todo", "", ("card", "CAP5")),
            ("RE at two", "re", 2, "", "todo", "", ("card", "FIL0")),
            ("RE below two", "re", 1, "", "todo", "", ("card", "CAP5")),
            ("override at cap", "build", 1, "try_cap: 1\n", "todo", "", ("card", "FIL0")),
            ("override below cap", "re", 2, "try_cap: 4\n", "todo", "", ("card", "CAP5")),
            ("capped resume", "build", 3, "", "in-progress", "", ("card", "FIL0")),
            ("capped branch landing", "build", 8, "", "parked", "built", ("land", "CAP5"))):
        with tempfile.TemporaryDirectory(prefix="loopctl_retrycap_") as d:
            fixture(d)
            card(d, "CAP5", 5, status, f"tries: {count}\nbranch: {branch}\n{override}resume: approach found X\n")
            run(d, ["set", "CAP5", f"phase={phase}"])
            # Also exhaust the legacy unchanged-card guard: landing must bypass both.
            if branch:
                import hashlib
                fp = hashlib.sha1(open(os.path.join(d, "cards", "CAP5.md"), "rb").read()).hexdigest()
                json.dump({"attempts": {"CAP5": {"n": 99, "fp": fp}}}, open(os.path.join(d, "state.json"), "w"))
            check("RETRYCAP " + label, d, want)
            if count >= (int(override.split(":")[1]) if override else (2 if phase == "re" else 3)) and not branch:
                brief = run(d, ["orient"], {"OPENBF_JOB_JSON": json.dumps({"job": "planner"})}).stdout
                ok = f"STUCK: CAP5 after {count} tries" in brief and "approach found X" in brief
                print(f"{'ok  ' if ok else 'FAIL'} RETRYCAP planner findings")
                bad += not ok
    with tempfile.TemporaryDirectory(prefix="loopctl_retrycap_finish_") as d:
        fixture(d)
        card(d, "CAP5", 5, "todo", "tries: 1\nresume: first approach X\n")
        job = {"job": "card", "card": "CAP5", "claim": "CAP5", "phase": "build", "attempt_id": "session1"}
        env = {"OPENBF_JOB_JSON": json.dumps(job)}
        run(d, ["finish-attempt", "--since", "1"], env)
        run(d, ["set", "CAP5", "resume=second approach Y"])
        run(d, ["postmortem", "--since", "1", "--pid", me], env)
        ok = read_card(os.path.join(d, "cards", "CAP5.md"))["tries"] == "2"
        print(f"{'ok  ' if ok else 'FAIL'} RETRYCAP clean finish plus postmortem counts once across resume edits")
        bad += not ok
        brief = run(d, ["orient"], env).stdout
        backlog = open(os.path.join(d, "BACKLOG.md")).read()
        ok = "Try 3 of 3: previous tries found: second approach Y" in brief and "tries 2/3" in backlog
        print(f"{'ok  ' if ok else 'FAIL'} RETRYCAP orient and backlog show cumulative tries and findings")
        bad += not ok
        job["attempt_id"] = "session2"
        run(d, ["postmortem", "--since", "2", "--pid", me], {"OPENBF_JOB_JSON": json.dumps(job)})
        c = read_card(os.path.join(d, "cards", "CAP5.md"))
        ok = c["tries"] == "3" and c["status"] == "blocked" and not c.get("needs") and c["resume"] == "second approach Y"
        print(f"{'ok  ' if ok else 'FAIL'} RETRYCAP cumulative cap preserves findings without the maintainer hold")
        bad += not ok
        for label, status, branch, kind in (("done", "done", "", "card"),
                                             ("land", "blocked", "built", "land"),
                                             ("ready branch", "parked", "built", "card")):
            card(d, "EXEMPT", 1, status, f"tries: 2\nbranch: {branch}\n")
            ex = {"job": kind, "card": "EXEMPT", "claim": "EXEMPT", "attempt_id": label}
            run(d, ["postmortem", "--since", "3", "--pid", me], {"OPENBF_JOB_JSON": json.dumps(ex)})
            ok = read_card(os.path.join(d, "cards", "EXEMPT.md"))["tries"] == "2"
            print(f"{'ok  ' if ok else 'FAIL'} RETRYCAP no failed try for {label}")
            bad += not ok
        card(d, "SLICE", 5, "in-progress", "tries: 2\nresume: slice 1\n")
        sl = {"job": "card", "card": "SLICE", "claim": "SLICE", "phase": "build", "attempt_id": "slice2"}
        run(d, ["landed", "SLICE", "abc12345"])
        run(d, ["set", "SLICE", "status=todo", "resume=slice 3 next"])
        run(d, ["postmortem", "--since", "4", "--pid", me], {"OPENBF_JOB_JSON": json.dumps(sl)})
        ok = read_card(os.path.join(d, "cards", "SLICE.md"))["tries"] == "0"
        print(f"{'ok  ' if ok else 'FAIL'} RETRYCAP a slice landed in the session and reopened is no failed try")
        bad += not ok
        card(d, "PHASE", 5, "todo", "tries: 1\n")
        run(d, ["set", "PHASE", "phase=build"])
        ph = {"job": "card", "card": "PHASE", "claim": "PHASE", "phase": "re", "attempt_id": "re-done"}
        run(d, ["postmortem", "--since", "5", "--pid", me], {"OPENBF_JOB_JSON": json.dumps(ph)})
        ok = read_card(os.path.join(d, "cards", "PHASE.md"))["tries"] == "0"
        print(f"{'ok  ' if ok else 'FAIL'} RETRYCAP an re session that moved the card to build is no failed try")
        bad += not ok
        for field in ("tries=wrong", "tries=-1", "try_cap=0", "try_cap=1.5"):
            card(d, "BAD", 1, "todo")
            run(d, ["set", "BAD", field])
            ok = run(d, ["check"]).returncode == 1
            print(f"{'ok  ' if ok else 'FAIL'} RETRYCAP check rejects {field}")
            bad += not ok
    return bad


def self_test_worktrees():
    """worktree_verdicts over fixture rows: what the `worktrees` sweep may remove and why it keeps the rest."""
    bad = 0
    cs = {"DONE": {"id": "DONE", "status": "done"}, "OPEN": {"id": "OPEN", "status": "blocked"}}
    row = lambda name, **kw: dict(dict(path=f"/wt/{name}", name=name, head="abc", branch=None, on_main=True, dirty=[], procs=[],
                                       created=100, exists=True), **kw)
    for name, r, live, since, mentions, want in (
            ("a detached scratch tree of a done card", row("done-main-scratch"), set(), {}, {}, True),
            ("its HEAD not on main", row("done-main-scratch", on_main=False), set(), {}, {}, False),
            ("a process runs in it", row("done-main-scratch", procs=[1]), set(), {}, {}, False),
            ("its card's claim is live", row("done-main-scratch"), {"DONE"}, {"DONE": 0}, {}, False),
            ("an open card names it", row("done-main-scratch"), set(), {}, {"done-main-scratch": "OPEN"}, False),
            ("a scratch tree of a card not done", row("open-x-scratch"), set(), {}, {}, False),
            ("a detached tree not named *-scratch", row("done-proto"), set(), {}, {}, False),
            ("an audit's tree made before the live audit's claim", row("audit-open-main-scratch"), {"AUDIT"}, {"AUDIT": 1000}, {}, True),
            ("an audit's tree made since the live audit's claim", row("audit-open-main-scratch", created=2000), {"AUDIT"}, {"AUDIT": 1000}, {}, False),
            ("a merged branch tree of a done card", row("done", branch="done"), set(), {}, {}, True),
            ("a branch tree whose card is not done", row("open", branch="open"), set(), {}, {}, False),
            ("a branch tree with no card", row("nocard", branch="nocard"), set(), {}, {}, False),
            ("a capture's render tree of a done card", row("done-render"), set(), {}, {}, True),
            ("a render tree of a card not done", row("open-render"), set(), {}, {}, False),
            ("a render tree with a process naming it", row("done-render", procs=[1]), set(), {}, {}, False)):
        got = worktree_verdicts([r], cs, live, since, mentions)[0]
        ok = got[1] == want
        bad += not ok
        print(f"{'ok  ' if ok else 'FAIL'} worktrees: {name}: {'candidate' if got[1] else 'kept'} ({got[2]})")
    return bad


def self_test_files():
    """`files:` entries (TGTC2): a short path and its full form are one file to the picker; `set` writes the full form;
    `check` warns of a path that exists nowhere unless it is marked `(new)`; a planner's `set` on another session's live
    in-progress card writes no `owner:`. Returns the failure count."""
    import tempfile
    bad = 0
    with tempfile.TemporaryDirectory(prefix="loopctl_files_selftest_") as d:
        for f in ("engine/crates/core/src/nav.rs", "scripts/x.py"):
            os.makedirs(os.path.dirname(os.path.join(d, f)), exist_ok=True); open(os.path.join(d, f), "w").write("")
        checks = [("a short path resolves under engine/crates", file_paths("core/src/nav.rs", d) == ["engine/crates/core/src/nav.rs"]),
                  ("a full path stays", file_paths("engine/crates/core/src/nav.rs (the graph)", d) == ["engine/crates/core/src/nav.rs"]),
                  ("prose in an entry is not a path", file_paths("the nav graph plumbing", d) == []),
                  ("a repo-root path stays", file_paths("scripts/x.py", d) == ["scripts/x.py"]),
                  ("normalise keeps the note", normalise_files("core/src/nav.rs (graph), scripts/x.py", d)
                   == "engine/crates/core/src/nav.rs (graph), scripts/x.py")]
        loop = os.path.join(d, "loop")
        os.makedirs(os.path.join(loop, "cards"))
        hdr = "---\nid: {0}\ntitle: t\nmilestone: M0\nvalue: 5\nphase: build\nstatus: {1}\nfiles: {2}\n---\n"
        open(os.path.join(loop, "cards", "AA.md"), "w").write(hdr.format("AA", "in-progress", "engine/crates/core/src/ai/steer.rs"))
        open(os.path.join(loop, "cards", "BB.md"), "w").write(hdr.format("BB", "todo", "core/src/ai/steer.rs"))
        open(os.path.join(loop, "cards", "CC.md"), "w").write(hdr.format("CC", "todo", "engine/crates/zz_nowhere.rs, engine/crates/zz_new.rs (new)"))
        for f in ("DD", "EE", "FF"):  # under 5 open cards the picker calls a planner
            open(os.path.join(loop, "cards", f"{f}.md"), "w").write(hdr.format(f, "todo", "").replace("value: 5", "value: 1"))
        open(os.path.join(loop, "STATUS.md"), "w").write("fixture\n")
        os.makedirs(os.path.join(loop, "locks", "AA")); open(os.path.join(loop, "locks", "AA", "pid"), "w").write(str(os.getpid()))
        env = dict(os.environ, OPENBF_LOOP_DIR=loop, OPENBF_CONTROL_DIR=loop, OPENBF_LEAD_TMP="")
        me = [sys.executable, os.path.abspath(__file__)]
        nxt = subprocess.run(me + ["next", "--lane", "1", "--lanes", "2", "--pid", str(os.getpid())], env=env, capture_output=True, text=True)
        try:
            got = json.loads(nxt.stdout).get("card")
        except ValueError:
            got = nxt.stdout
        checks.append(("a short-path card waits for the live card on the same file", got == "CC"))
        chk = subprocess.run(me + ["check"], env=env, capture_output=True, text=True).stdout
        checks.append(("check warns of a path that exists nowhere", "zz_nowhere.rs exists nowhere" in chk))
        checks.append(("check is quiet about a (new) file", "zz_new.rs" not in chk))
        subprocess.run(me + ["set", "BB", "files=core/src/ai/steer.rs"], env=env, capture_output=True, text=True)
        checks.append(("set writes the full path", "files: engine/crates/core/src/ai/steer.rs\n" in open(os.path.join(loop, "cards", "BB.md")).read()))
        pe = dict(env, OPENBF_JOB_JSON=json.dumps({"claim": "PLANNER"}), OPENBF_LANE_PID=str(os.getpid()))
        subprocess.run(me + ["set", "AA", "resume=x"], env=pe, capture_output=True, text=True)
        checks.append(("a planner's set on a live card writes no owner", "owner:" not in open(os.path.join(loop, "cards", "AA.md")).read()))
        shutil_rm = __import__("shutil").rmtree; shutil_rm(os.path.join(loop, "locks", "AA"))
        subprocess.run(me + ["set", "AA", "resume=y"], env=pe, capture_output=True, text=True)
        checks.append(("... and does once that claim is gone", "owner: PLANNER" in open(os.path.join(loop, "cards", "AA.md")).read()))
    for label, ok in checks:
        bad += not ok
        print(f"{'ok  ' if ok else 'FAIL'} files: {label}")
    return bad


def self_test_effort():
    """A card's own `effort:` beats the phase default; an unknown value is ignored."""
    import tempfile
    bad = 0
    for name, extra, want in (("re card, no effort: -> xhigh (the phase default)", "", "xhigh"),
                              ("re card, effort: high -> high", "effort: high\n", "high"),
                              ("re card, effort: bogus -> xhigh", "effort: bogus\n", "xhigh")):
        with tempfile.TemporaryDirectory(prefix="loopctl_selftest_") as d:
            os.makedirs(os.path.join(d, "cards"))
            open(os.path.join(d, "cards", "RE5.md"), "w").write(
                f"---\nid: RE5\ntitle: t\nmilestone: M1\nvalue: 5\nphase: re\nstatus: todo\n{extra}---\n")
            for i in range(5):  # fewer than 5 open cards would make the planner the job
                open(os.path.join(d, "cards", f"FIL{i}.md"), "w").write(
                    f"---\nid: FIL{i}\ntitle: t\nmilestone: M1\nvalue: 1\nphase: build\nstatus: todo\n---\n")
            open(os.path.join(d, "STATUS.md"), "w").write("fixture\n")
            env = dict(os.environ, OPENBF_LOOP_DIR=d, OPENBF_CONTROL_DIR=d, OPENBF_LEAD_TMP="")
            out = subprocess.run([sys.executable, os.path.abspath(__file__), "next", "--lane", "1", "--lanes", "1",
                                  "--pid", str(os.getpid())], env=env, capture_output=True, text=True)
            try:
                got = json.loads(out.stdout).get("effort")
            except Exception:
                got = f"<{out.stdout.strip()} {out.stderr.strip()}>"
            ok = got == want
            bad += not ok
            print(f"{'ok  ' if ok else 'FAIL'} {name}: {got}")
    return bad


def self_test_lane3():
    """Lane 3: its kinds before a higher-value build card; the periodic audit goes to lane 3 while its launcher lives."""
    import tempfile
    bad = 0
    fixture = [("BLD5", 5, "build"), ("RE4", 4, "re"), ("CAP3", 3, "capture"), ("CLN5", 5, "cleanup"), ("BLD4", 4, "build")]
    # a live process holding a second thread, its command line the "launcher's" (loopctl.py, as this self-test's own): the
    # thread's id answers kill(0) and its /proc cmdline matches - only its Tgid tells it is not a process (LOOPT)
    helper = subprocess.Popen([sys.executable, "-c", "import threading, time, sys\nt = threading.Thread(target=time.sleep, args=(60,), daemon=True)\n"
                               "t.start(); print(t.native_id, flush=True); time.sleep(60)", "loopctl.py"], stdout=subprocess.PIPE, text=True)
    other = subprocess.Popen(["sleep", "60"])  # a live process that is not the launcher (a reused pid)
    tid = helper.stdout.readline().strip()
    try:
        bad += self_test_lane3_cases(fixture, tid, str(other.pid))
    finally:
        for x in (helper, other):
            x.kill(); x.wait()
    return bad


def self_test_lane3_cases(fixture, tid, other):
    import tempfile
    bad = 0
    for name, heartbeat, steps in [
            ("lane 3 alive: lanes 1-2 leave the audit to it", str(os.getpid()),
             [(1, "card", "BLD5"), (3, "audit", None), (3, "card", "RE4"), (3, "card", "CAP3"), (2, "card", "CLN5")]),
            ("lane 3 dead: lane 1 takes the audit", "999999999", [(1, "audit", None), (3, "card", "RE4")]),
            ("lane 3's file holds a live process's THREAD id: dead, lane 1 takes the audit", tid, [(1, "audit", None)]),
            ("lane 3's file holds a live process that is not the launcher: dead", other, [(1, "audit", None)]),
            ("no lane 3 ever: lane 2 takes the audit", None, [(2, "audit", None)])]:
        with tempfile.TemporaryDirectory(prefix="loopctl_selftest_") as d:
            os.makedirs(os.path.join(d, "cards"))
            for cid, v, ph in fixture:
                open(os.path.join(d, "cards", f"{cid}.md"), "w").write(
                    f"---\nid: {cid}\ntitle: t\nmilestone: M1\nvalue: {v}\nphase: {ph}\nstatus: todo\n---\n")
            open(os.path.join(d, "STATUS.md"), "w").write("fixture\n")
            json.dump({"sessions_since_audit": AUDIT_EVERY_SESSIONS, "last_job": "card"}, open(os.path.join(d, "state.json"), "w"))
            if heartbeat:
                os.makedirs(os.path.join(d, "lanes"))
                open(os.path.join(d, "lanes", "lane3.pid"), "w").write(heartbeat)
            env = dict(os.environ, OPENBF_LOOP_DIR=d, OPENBF_CONTROL_DIR=d, OPENBF_LEAD_TMP="", OPENBF_LAUNCHER_MATCH="loopctl.py")
            for lane, want_job, want_card in steps:
                out = subprocess.run([sys.executable, os.path.abspath(__file__), "next", "--lane", str(lane),
                                      "--lanes", "1" if lane == 3 else "2", "--pid", str(os.getpid())],
                                     env=env, capture_output=True, text=True)
                try:
                    j = json.loads(out.stdout)
                    got = (j.get("job"), j.get("card"))
                except Exception:
                    got = f"<{out.stdout.strip()} {out.stderr.strip()}>"
                ok = got == (want_job, want_card)
                bad += not ok
                print(f"{'ok  ' if ok else 'FAIL'} {name}: lane {lane} -> {got} (want {(want_job, want_card)})")
    return bad


def self_test_commit_docs():
    """commit-docs in a temp git repo: refuses a non-doc path, a held MAIN and a staged index; commits exactly the named
    doc (another dirty doc stays dirty) and, for an re-*.md, the regenerated analysis/INDEX.md, with the attribution
    line, and releases MAIN. Returns the failure count."""
    import tempfile
    with tempfile.TemporaryDirectory(prefix="loopctl_docs_selftest_") as d:
        repo, loop = os.path.join(d, "repo"), os.path.join(d, "loop")
        main_lock = os.path.join(loop, "locks", "MAIN")
        os.makedirs(os.path.join(repo, "analysis")); os.makedirs(os.path.join(loop, "locks"))
        g = lambda *a: git(*a, repo=repo)
        g("init", "-q", "-b", "main"); g("config", "user.email", "t@t"); g("config", "user.name", "t")
        open(os.path.join(repo, "README"), "w").write("r\n"); g("add", "README"); g("commit", "-q", "-m", "init")
        for f, t in (("analysis/re-x.md", "doc\n"), ("analysis/re-y.md", "draft\n"), ("code.rs", "x\n")):
            open(os.path.join(repo, f), "w").write(t)
        env = dict(os.environ, OPENBF_LOOP_DIR=loop, OPENBF_CONTROL_DIR=loop, OPENBF_LEAD_TMP="", OPENBF_REPO=repo)
        cmd = lambda *a: subprocess.run([sys.executable, os.path.abspath(__file__), "commit-docs", *a], env=env,
                                        capture_output=True, text=True, cwd=repo).returncode
        n_commits = lambda: int(g("rev-list", "--count", "HEAD").stdout.strip())
        checks = [("a non-doc path is refused", cmd("analysis/re-x.md", "code.rs", "-m", "m") == 2 and n_commits() == 1),
                  ("a path outside the repo is refused", cmd("../x/analysis/a.md", "-m", "m") == 2)]
        os.mkdir(main_lock); open(os.path.join(main_lock, "pid"), "w").write(str(os.getpid()))
        checks.append(("a held MAIN is refused", cmd("analysis/re-x.md", "-m", "m") == 3 and n_commits() == 1))
        os.remove(os.path.join(main_lock, "pid")); os.rmdir(main_lock)
        g("checkout", "-q", "-b", "card")
        checks.append(("a checkout off main is refused", cmd("analysis/re-x.md", "-m", "m") == 2 and n_commits() == 1))
        g("checkout", "-q", "main")
        g("add", "code.rs")
        checks.append(("a staged index is refused and MAIN let go", cmd("analysis/re-x.md", "-m", "m") == 2
                       and n_commits() == 1 and not os.path.exists(main_lock)))
        g("reset", "-q", "--", "code.rs")
        rc = cmd("analysis/re-x.md", "-m", "Docs: re-x")
        files = g("show", "--name-only", "--format=", "HEAD").stdout.split()
        index = open(os.path.join(repo, "analysis/INDEX.md")).read() if os.path.exists(os.path.join(repo, "analysis/INDEX.md")) else ""
        checks += [("the named doc is committed, and the regenerated RE index with it",
                    rc == 0 and n_commits() == 2 and sorted(files) == ["analysis/INDEX.md", "analysis/re-x.md"] and "re-x.md" in index),
                   ("the commit carries the attribution", ATTRIBUTION in g("log", "-1", "--format=%B").stdout),
                   ("another dirty doc and the code stay dirty",
                    sorted(g("status", "--porcelain").stdout.split()) == ["??", "??", "analysis/re-y.md", "code.rs"]),
                   ("MAIN is released", not os.path.exists(main_lock)),
                   ("a clean doc is refused", cmd("analysis/re-x.md", "-m", "m") == 2)]
    bad = 0
    for label, ok in checks:
        bad += not ok
        print(f"{'ok  ' if ok else 'FAIL'} commit-docs: {label}")
    return bad


def self_test_postmortem():
    """A scripted dead session in a temp loop dir (OPENBF_LOOP_DIR): its claim held by the lane's pid, its card
    in-progress, no report -> one DIED line and the claim freed; a session that wrote its report -> no line."""
    import tempfile
    with tempfile.TemporaryDirectory(prefix="loopctl_pm_selftest_") as d:
        for sub in ("cards", "reports", "locks/ZZDEAD"):
            os.makedirs(os.path.join(d, sub))
        for cid in ("ZZDEAD", "ZZOK"):
            open(os.path.join(d, "cards", f"{cid}.md"), "w").write(
                f"---\nid: {cid}\ntitle: t\nmilestone: M0\nvalue: 1\nphase: build\nstatus: in-progress\n---\n")
        open(os.path.join(d, "locks", "ZZDEAD", "pid"), "w").write(str(os.getpid()))
        open(os.path.join(d, "JOURNAL.md"), "w").write("# j\n")
        since = time.time() - 1
        open(os.path.join(d, "reports", "20260101_0000_card_ZZOK.md"), "w").write("r\n")
        open(os.path.join(d, "reports", "20260101_0001_card_ZZDEAD2.md"), "w").write("r\n")  # another card's: not ZZDEAD's
        env = dict(os.environ, OPENBF_LOOP_DIR=d, OPENBF_CONTROL_DIR=d, OPENBF_LEAD_TMP="")
        for cid in ("ZZDEAD", "ZZOK"):
            job = {"job": "card", "card": cid, "phase": "build", "claim": cid, "lane": 2}
            subprocess.run([sys.executable, os.path.abspath(__file__), "postmortem", "--since", str(since), "--pid",
                            str(os.getpid()), "--lane", "2"], env=dict(env, OPENBF_JOB_JSON=json.dumps(job)), capture_output=True)
        died = [l for l in open(os.path.join(d, "JOURNAL.md")).read().splitlines() if l.startswith("- DIED")]
        checks = [("a dead session gets one DIED line",
                   len(died) == 1 and "lane 2: job card card ZZDEAD phase build, claim ZZDEAD left (freed)" in died[0]),
                  ("its claim is freed", not os.path.exists(os.path.join(d, "locks", "ZZDEAD"))),
                  ("a session with its report gets none", not any("ZZOK" in l for l in died)),
                  ("dead and reported unfinished sessions both count", all(
                      read_card(os.path.join(d, "cards", f"{cid}.md")).get("tries") == "1" for cid in ("ZZDEAD", "ZZOK")))]
    bad = 0
    for label, ok in checks:
        bad += not ok
        print(f"{'ok  ' if ok else 'FAIL'} postmortem: {label}" + ("" if ok else f" ({died})"))
    return bad


def self_test_audits():
    """Audit by risk over fixture sources: risk domains first; a render target is skipped unless the 1-in-3 sample is due."""
    log = audit_log("- AUDIT 2026-01-01 VX: PARTLY — x\n  domain: vehicle\n- AUDIT 2026-01-02 RX-RE: OK — x\n  domain: render\n"
                    "- AUDIT 2026-01-03 AX + BX: OK — x\n")
    src = [("2026-02-03", "REND", "render", False, "w"), ("2026-02-02", "TOOL", "tooling", False, "w"),
           ("2026-02-01", "COLL", "collision", True, "w")]
    after_render = audit_candidates(src, log[:2])                        # the last two audits include a render one
    sampled = audit_candidates(src, log[:1] + [("AX", "OK", "sim")])     # neither of the last two was render
    checks = [
        ("audits.md parses targets, verdicts and domain lines",
         log == [("VX", "PARTLY", "vehicle"), ("RX-RE", "OK", "render"), ("AX + BX", "OK", "?")]),
        ("audited ids split a joint target and drop -RE", audited_ids(log) == {"VX", "RX", "AX", "BX"}),
        ("a risk domain ranks first", [t for t, *_ in after_render] == ["COLL", "TOOL", "REND"]),
        ("a render audit is skipped unless sampled", [ok for *_, ok in after_render] == [True, True, False]),
        ("a render audit is picked when the sample is due", [ok for *_, ok in sampled] == [True, True, True]),
        ("the domain guess puts vehicles before render", guess_domain("hover vehicle lighting") == "vehicle"
         and guess_domain("water colour") == "render" and guess_domain("soldier walls push", "M3") == "collision"
         and guess_domain("loopctl capture cards", "M0") == "tooling" and guess_domain("lower walls missing", "M1") == "render"),
    ]
    bad = 0
    for label, ok in checks:
        bad += not ok
        print(f"{'ok  ' if ok else 'FAIL'} audits: {label}")
    return bad


def self_test_audit_holds():
    """Real checkpoint/reset CLI plus the supervisor's pick and worker briefing, entirely in fixture directories."""
    import tempfile
    from unittest.mock import patch
    checks = []
    with tempfile.TemporaryDirectory(prefix="loopctl_audit_holds_") as d:
        analysis = os.path.join(d, "analysis")
        os.makedirs(analysis)
        os.makedirs(os.path.join(d, "cards"))
        for i in range(5):
            open(os.path.join(d, "cards", f"FILL{i}.md"), "w").write(
                f"---\nid: FILL{i}\ntitle: fixture\nstatus: blocked\nphase: build\nmilestone: M0\nvalue: 1\n---\n")
        sources = [("2026-01-03", "re-risk.md", "sim", False, "fixture"),
                   ("2026-01-02", "re-tool.md", "tooling", False, "fixture"),
                   ("2026-01-01", "re-render.md", "render", False, "fixture")]
        sources += [("2026-01-01", f"re-extra{i}.md", "tooling", False, "fixture") for i in range(3)]
        for _, target, _, _, _ in sources:
            open(os.path.join(analysis, target), "w").write("# OPUS RE (2026-01-01) - [O] UNAUDITED\n")
        ledger = os.path.join(d, "audits.md")
        ledger_bytes = "- AUDIT 2026-01-01 OLD: OK — fixture\n  domain: sim\n".encode()
        open(ledger, "wb").write(ledger_bytes)
        open(os.path.join(d, "report.md"), "w").write("incomplete fixture audit\n")
        open(os.path.join(d, "STATUS.md"), "w").write("fixture\n")
        state_path = os.path.join(d, "state.json")
        state_bytes = b'{"sessions_since_audit":10,"last_job":"planner"}'
        open(state_path, "wb").write(state_bytes)
        env = dict(os.environ, OPENBF_LOOP_DIR=d, OPENBF_CONTROL_DIR=d, OPENBF_LEAD_TMP="",
                   OPENBF_AUDIT_ROOT=d)

        def cli(*args):
            r = subprocess.run([sys.executable, os.path.abspath(__file__), *args], env=env,
                               capture_output=True, text=True)
            if r.returncode:
                raise AssertionError(r.stderr)
            return r.stdout

        def checkpoint(target, revision="fixture-rev"):
            cli("audit-checkpoint", target, "--missing", "input-a", "--missing", "input-b",
                "--revision", revision, "--reason", "primary inputs absent", "--report", "report.md")

        with patch.dict(globals(), AUDIT_HOLDS=os.path.join(d, "audit_holds.json"), AUDIT_ROOT=d,
                        AUDITS=ledger, ANALYSIS=analysis, CARDS=os.path.join(d, "cards")):
            baseline = audit_candidates(sources, [])
            checkpoint("re-risk.md")
            first = lambda: next((t for t, _, _, ok in audit_candidates(sources, []) if ok), None)
            checks.append(("unchanged missing inputs skip repeated target and pick next ready", first() == "re-tool.md"))
            checkpoint("re-risk.md", "new-source-rev")
            checks.append(("new source revision cannot restore inputs", first() == "re-tool.md"))
            open(os.path.join(d, "input-a"), "w").close()
            checks.append(("one of two inputs restored still skips", first() == "re-tool.md"))
            cli("audit-checkpoint", "re-risk.md", "--missing", "input-b", "--revision", "narrower-rev",
                "--reason", "remaining absent input", "--report", "report.md")
            checks.append(("later checkpoint retains earlier prerequisites until reviewed reset",
                           audit_holds()["re-risk.md"]["missing"] == ["input-a", "input-b"]))
            open(os.path.join(d, "input-b"), "w").close()
            checks.append(("both restored retry in original risk order", audit_candidates(sources, []) == baseline))
            os.remove(os.path.join(d, "input-a")); os.remove(os.path.join(d, "input-b"))
            for _, target, _, _, _ in sources[1:]:
                checkpoint(target)
            briefing = "\n".join(audit_briefing())
            checks.append(("all held briefing lists every blocker without a pick",
                           "INCOMPLETE COVERAGE" in briefing and "- pick" not in briefing
                           and all(t in briefing for _, t, *_ in sources)
                           and all(p in briefing for p in ("input-a", "input-b", "audit-reset"))))
            # The actual pick() CLI is what the supervisor imports; no production claims/state are touched.
            job = json.loads(cli("next", "--pid", str(os.getpid())))
            checks.append(("all held supervisor waits without claiming an audit or resetting cadence",
                           job["job"] == "none" and "Incomplete audit coverage" in job["reason"]
                           and not os.path.exists(os.path.join(d, "locks", "AUDIT"))
                           and open(state_path, "rb").read() == state_bytes))
            # Exercise audit worker orientation with the same shared fixture checkpoint store.
            r = subprocess.run([sys.executable, os.path.abspath(__file__), "orient"],
                               env=dict(env, OPENBF_JOB_JSON='{"job":"audit"}'), capture_output=True, text=True)
            checks.append(("worker orientation reports hold without a verdict",
                           r.returncode == 0 and "INCOMPLETE COVERAGE" in r.stdout and "- pick" not in r.stdout))
            open(os.path.join(d, "cards", "READY.md"), "w").write(
                "---\nid: READY\ntitle: fixture\nstatus: todo\nphase: build\nmilestone: M0\nvalue: 1\n---\n")
            ready = json.loads(cli("next", "--pid", str(os.getpid())))
            checks.append(("held audit does not starve eligible card work and carries restoration reason",
                           ready.get("card") == "READY" and "Incomplete audit coverage" in ready["reason"]
                           and not os.path.exists(os.path.join(d, "locks", "AUDIT"))))
            before = audit_holds()
            refused = subprocess.run([sys.executable, os.path.abspath(__file__), "audit-reset", "re-risk.md",
                                      "--reason", "unreviewed"], env=env, capture_output=True, text=True)
            checks.append(("reset without review is refused without changing holds",
                           refused.returncode != 0 and audit_holds() == before))
            cli("audit-reset", "re-risk.md", "--reviewed-by", "fixture reviewer", "--reason", "reviewed changed scope")
            checks.append(("reviewed reset removes only named hold",
                           audit_holds() == {t: h for t, h in before.items() if t != "re-risk.md"}
                           and first() == "re-risk.md"))
            checks.append(("checkpoint/reset leave exact audit ledger bytes intact",
                           open(ledger, "rb").read() == ledger_bytes))
            checks.append(("holds preserve render sample and risk order",
                           [t for t, *_ in audit_candidates(sources, [], {})] == [t for t, *_ in baseline]
                           and not audit_candidates(sources, [("OLD", "OK", "render")], {})[-1][3]))
    for label, ok in checks:
        print(f"{'ok  ' if ok else 'FAIL'} audit holds: {label}")
    return sum(not ok for _, ok in checks)


def self_test_claims():
    """claim() on a pid-less lock dir: held while fresh (a claimer between mkdir and write), taken once stale — the old
    alive("0") read os.kill(0, 0), which always succeeds, so such a dir was held forever. Returns the failure count."""
    import tempfile
    global LOCKS
    saved, bad = LOCKS, 0
    with tempfile.TemporaryDirectory(prefix="loopctl_claim_selftest_") as d:
        LOCKS = d
        try:
            os.mkdir(os.path.join(d, "ZZ"))
            checks = [("a fresh pid-less claim is held", not claim("ZZ", os.getpid()))]
            os.utime(os.path.join(d, "ZZ"), (time.time() - 2 * CLAIM_WRITE_GRACE,) * 2)
            checks.append(("a stale pid-less claim is taken", claim("ZZ", os.getpid())))
            checks.append(("a live owner's claim is held", not claim("ZZ", 1)))
            checks.append(("pid 0 is not alive", not alive("0")))
        finally:
            LOCKS = saved
    for label, ok in checks:
        bad += not ok
        print(f"{'ok  ' if ok else 'FAIL'} claims: {label}")
    return bad


def self_test_targets():
    """The target sweep over a fixture lead_tmp (TGTC): `targets --yes` removes exactly the done-card, parked-card,
    scratch, orphan and branchless-todo targets; the live-claim, in-progress, todo-card-with-a-branch, busy and symlinked
    ones stay, and the dry run lists exactly the ones --yes removes. Returns the failure count."""
    import tempfile
    fixture = [  # (target name, card (id, status) or None, removed?)
        ("zzlive", ("ZZLIVE", "todo"), False),     # its card holds a live claim (this process)
        ("zzlive-ctl-scratch", None, False),       # a scratch target of the live card: its stem is protected
        ("zzdone", ("ZZDONE", "done"), True),
        ("zzpark", ("ZZPARK", "parked"), True),
        ("zzuaud-scratch", None, True),
        ("zzold_pxc", None, True),
        ("zzorphan", None, True),                  # no card, no branch, no worktree
        ("zzwip", ("ZZWIP", "in-progress"), False),  # claim not live: the next session resumes it
        ("zztodo", ("ZZTODO", "todo"), True),      # a todo card with no branch or worktree: nothing to resume
        ("zzbranch", ("ZZBRANCH", "todo"), False),  # a todo card whose branch exists (the fixture repo below)
        ("zzbusy", ("ZZBUSY", "done"), False),     # dead, but a process runs inside it (a gate still using it)
        # TGTC2's name forms: a capture's <card>-render, an audit's audit-<card>-*-scratch, a busy path by command line
        ("zzrdone-render", ("ZZRDONE", "done"), True),
        ("zzrpark-render", ("ZZRPARK", "parked"), True),
        ("zzrtodo-render", ("ZZRTODO", "todo"), False),  # its card may still be captured
        ("zzrgone-render", None, True),                  # no such card
        ("zzlive-render", None, False),                  # the live card's capture
        ("audit-zzrdone-main-scratch", None, False),     # the AUDIT claim is live (this process)
        ("zzargv-scratch", None, False),                 # dead, but a process names a path inside it on its command line
    ]
    with tempfile.TemporaryDirectory(prefix="loopctl_tgt_selftest_") as d:
        lt = os.path.join(d, "lead_tmp")
        os.makedirs(os.path.join(d, "cards"))
        for lk in ("ZZLIVE", "AUDIT"):
            os.makedirs(os.path.join(d, "locks", lk)); open(os.path.join(d, "locks", lk, "pid"), "w").write(str(os.getpid()))
        for name, card, _ in fixture:
            os.makedirs(os.path.join(lt, f"{name}_target", "release"))
            open(os.path.join(lt, f"{name}_target", "release", "game"), "w").write("x")
            if card:
                open(os.path.join(d, "cards", f"{card[0]}.md"), "w").write(
                    f"---\nid: {card[0]}\ntitle: t\nmilestone: M0\nvalue: 1\nphase: build\nstatus: {card[1]}\nbranch: {name}\n---\n")
        os.makedirs(os.path.join(d, "elsewhere")); os.symlink(os.path.join(d, "elsewhere"), os.path.join(lt, "zzlink_target"))
        nested = [  # (a cargo target inside lt/<card>/, removed by the sweep?) - card WEARY
            ("zzdone/mtgt", True), ("zzpark/a/b/target", True),  # a done / parked card's, three levels down
            ("zzlive/mtgt", False), ("zzwip/mtgt", False),        # a live claim's, an in-progress card's
            ("zzbranch/x/target", False),                         # a todo card's (until the `set` below parks it)
            ("zzorphan/mtgt", False),                             # no card of that name: left to a person
            ("zzdone/a/b/c/target", False)]                       # deeper than three levels
        for n, _ in nested:
            os.makedirs(os.path.join(lt, n, "debug")); open(os.path.join(lt, n, "CACHEDIR.TAG"), "w").write("x")
        open(os.path.join(lt, "zzdone", "notes.txt"), "w").write("x")  # the scratch dir's own files stay
        os.makedirs(os.path.join(d, "elsewhere", "t")); open(os.path.join(d, "elsewhere", "t", "CACHEDIR.TAG"), "w").write("x")
        os.makedirs(os.path.join(lt, "zzpark", "lnk")); os.rmdir(os.path.join(lt, "zzpark", "lnk"))
        os.symlink(os.path.join(d, "elsewhere"), os.path.join(lt, "zzpark", "lnk"))  # a link inside: not followed
        repo = os.path.join(d, "repo")  # the branch and worktree names the verdicts read: one branch, zzbranch
        subprocess.run(["git", "init", "-q", "-b", "main", repo])
        subprocess.run(["git", "-C", repo, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "t"])
        subprocess.run(["git", "-C", repo, "branch", "zzbranch"])
        env = dict(os.environ, OPENBF_LOOP_DIR=d, OPENBF_CONTROL_DIR=d, OPENBF_LEAD_TMP=lt, OPENBF_REPO=repo)
        me = [sys.executable, os.path.abspath(__file__)]
        busy = subprocess.Popen(["sleep", "60"], cwd=os.path.join(lt, "zzbusy_target", "release"))
        argv = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)",
                                 f"--target-dir={os.path.join(lt, 'zzargv-scratch_target', 'release')}"], cwd=d)
        try:  # the dry run's rows are deleted by cleanse_delete.sh without a second look: a target in use is not one
            dry = subprocess.run(me + ["targets"], env=env, capture_output=True, text=True).stdout
            subprocess.run(me + ["targets", "--yes"], env=env, capture_output=True, text=True)
        finally:
            for b in (busy, argv):
                b.kill(); b.wait()
        listed = sorted(os.path.basename(l.split("\t")[0]) for l in dry.splitlines() if "\t" in l and "_target\t" in l)
        want = sorted(f"{n}_target" for n, _, gone in fixture if gone)
        gone = sorted(f"{n}_target" for n, _, _ in fixture if not os.path.exists(os.path.join(lt, f"{n}_target")))
        n_listed = sorted(os.path.relpath(l.split("\t")[0], lt) for l in dry.splitlines() if "nested target" in l)
        n_want = sorted(n for n, rm in nested if rm)
        n_gone = sorted(n for n, _ in nested if not os.path.exists(os.path.join(lt, n)))
        kept_files = os.path.isfile(os.path.join(lt, "zzdone", "notes.txt")) and os.path.isdir(os.path.join(d, "elsewhere", "t"))
        # `set <ID> status=parked` removes that card's own target at once
        subprocess.run(me + ["set", "ZZBRANCH", "status=parked"], env=env, capture_output=True, text=True)
        parked = not os.path.exists(os.path.join(lt, "zzbranch_target")) and not os.path.exists(os.path.join(lt, "zzbranch/x/target"))
        link_ok = os.path.islink(os.path.join(lt, "zzlink_target")) and os.path.isdir(os.path.join(d, "elsewhere"))
    bad = 0
    for label, ok, detail in (("dry run lists exactly the dead targets", listed == want, listed),
                              ("--yes removes exactly those", gone == want, gone),
                              ("dry run lists exactly the dead nested targets", n_listed == n_want, f"{n_listed}, want {n_want}"),
                              ("--yes removes exactly those, not the dir's files nor a link's referent", n_gone == n_want and kept_files,
                               f"{n_gone}, files kept {kept_files}, want {n_want}"),
                              ("set status=parked removes the card's target and its nested ones", parked, ""),
                              ("a symlinked *_target and its referent are untouched", link_ok, "")):
        bad += not ok
        print(f"{'ok  ' if ok else 'FAIL'} targets: {label}" + ("" if ok else f" (got {detail}, want {want})"))
    return bad


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sp = ap.add_subparsers(dest="cmd", required=True)
    n = sp.add_parser("next"); n.add_argument("--lane", type=int, default=1); n.add_argument("--lanes", type=int, default=1); n.add_argument("--pid", default=str(os.getppid()))
    o = sp.add_parser("orient"); o.add_argument("--job-json")
    s = sp.add_parser("start"); s.add_argument("id")
    st = sp.add_parser("set"); st.add_argument("id"); st.add_argument("kv", nargs="+")
    nw = sp.add_parser("new"); nw.add_argument("id"); nw.add_argument("--title", required=True); nw.add_argument("--milestone", required=True)
    nw.add_argument("--value", required=True); nw.add_argument("--phase", required=True, choices=PHASES); nw.add_argument("--deps", default="")
    nw.add_argument("--files", default=""); nw.add_argument("--declares", default=""); nw.add_argument("--body", default="")
    r = sp.add_parser("release"); r.add_argument("name")
    sc = sp.add_parser("sweep-claims"); sc.add_argument("--pid", required=True)
    cl = sp.add_parser("claim"); cl.add_argument("name"); cl.add_argument("--pid", required=True)
    tg = sp.add_parser("targets"); tg.add_argument("--yes", action="store_true")
    wk = sp.add_parser("worktrees"); wk.add_argument("--yes", action="store_true")
    sp.add_parser("backlog"); sp.add_parser("check"); sp.add_parser("scoreboard"); sp.add_parser("self-test"); sp.add_parser("approvals")
    sp.add_parser("audit-status")
    ac = sp.add_parser("audit-checkpoint"); ac.add_argument("target")
    ac.add_argument("--missing", action="append", required=True)
    ac.add_argument("--revision", required=True); ac.add_argument("--reason", required=True); ac.add_argument("--report", required=True)
    ar = sp.add_parser("audit-reset"); ar.add_argument("target")
    ar.add_argument("--reviewed-by", required=True); ar.add_argument("--reason", required=True)
    ld = sp.add_parser("landed"); ld.add_argument("id"); ld.add_argument("sha")
    cd = sp.add_parser("commit-docs"); cd.add_argument("paths", nargs="+"); cd.add_argument("-m", "--message", required=True)
    fa = sp.add_parser("finish-attempt"); fa.add_argument("--since", type=float, required=True)
    pm = sp.add_parser("postmortem"); pm.add_argument("--since", type=float, required=True); pm.add_argument("--pid", required=True)
    pm.add_argument("--lane", default="?")
    ca = sp.add_parser("check-absence"); ca.add_argument("doc"); ca.add_argument("--warn", action="store_true")
    ca.add_argument("--strict", action="store_true"); ca.add_argument("--base")
    me = sp.add_parser("metrics"); me.add_argument("--csv", action="store_true"); me.add_argument("--out")
    a = ap.parse_args()

    if a.cmd == "next":
        print(json.dumps(pick(a.lane, a.lanes, a.pid)), flush=True)
        if LEAD_TMP:  # the dead-target sweep runs detached: a 45 GB rmtree must not hold up the session's start
            log = open(os.path.join(LOOP, "targets.log"), "a")
            log.write(f"-- {datetime.datetime.now():%Y-%m-%d %H:%M:%S} sweep from next (lane {a.lane})\n"); log.flush()
            subprocess.Popen([sys.executable, os.path.abspath(__file__), "targets", "--yes"], stdout=log, stderr=log,
                             stdin=subprocess.DEVNULL, start_new_session=True)
    elif a.cmd == "targets":
        sweep_targets(a.yes)
    elif a.cmd == "worktrees":
        sys.exit(sweep_worktrees(a.yes))
    elif a.cmd == "orient":
        job = json.load(open(a.job_json)) if a.job_json else json.loads(os.environ.get("OPENBF_JOB_JSON", "{}") or "{}")
        print(orient(job))
    elif a.cmd == "start":
        start(a.id)
    elif a.cmd == "audit-status":
        print("\n".join(audit_briefing()))
    elif a.cmd == "audit-checkpoint":
        audit_checkpoint(a.target, a.missing, a.revision, a.reason, a.report)
        print(f"{a.target}: incomplete checkpoint recorded; remains UNAUDITED")
    elif a.cmd == "audit-reset":
        audit_reset(a.target, a.reviewed_by, a.reason)
        print(f"{a.target}: reviewed prerequisite hold cleared; remains UNAUDITED")
    elif a.cmd == "set":
        cs = cards()
        if a.id not in cs:
            sys.exit(f"no card {a.id}")
        c = cs[a.id]
        for kv in a.kv:
            k, _, v = kv.partition("=")
            if k == "status" and v not in STATUSES:
                sys.exit(f"status must be one of {STATUSES}")
            if k == "phase" and v not in PHASES:
                sys.exit(f"phase must be one of {PHASES}")
            c[k] = normalise_files(v) if k == "files" else v
        note_owner(c)
        write_card(c); write_backlog()
        print(f"{a.id}: " + ", ".join(a.kv))
        if c.get("status") in ("parked", "done", "blocked"):  # its target is dead now; a resume re-seeds it by reflink
            remove_target(card_target(c))
            for n in {c["id"].lower(), (c.get("branch") or "").lower()} - {""}:  # and those in its scratch dir (WEARY)
                for d in nested_targets(LEAD_TMP, n) if LEAD_TMP else []:
                    remove_target(d)
    elif a.cmd == "new":
        os.makedirs(CARDS, exist_ok=True)
        p = os.path.join(CARDS, f"{a.id}.md")
        if os.path.exists(p):
            sys.exit(f"card {a.id} exists")
        c = dict(id=a.id, title=a.title, milestone=a.milestone, value=a.value, phase=a.phase, status="todo", deps=a.deps,
                 files=normalise_files(a.files), declares=a.declares, branch="", resume="", created=f"{datetime.date.today()}", _path=p,
                 _body=a.body or "\n## Why\n\n## Evidence\n\n## Acceptance\n\n## Notes\n")
        write_card(c); write_backlog()
        print(p)
    elif a.cmd == "release":
        release(a.name); print(f"released {a.name}")
    elif a.cmd == "sweep-claims":
        sweep_claims(a.pid)
    elif a.cmd == "claim":
        ok = claim(a.name, a.pid); print("claimed" if ok else "HELD by a live session"); sys.exit(0 if ok else 1)
    elif a.cmd == "approvals":
        print("\n".join(approvals_lines()) or "no approvals in force")
    elif a.cmd == "scoreboard":
        print(scoreboard())
    elif a.cmd == "backlog":
        write_backlog(); print(os.path.join(LOOP, "BACKLOG.md"))
    elif a.cmd == "check":
        bad = warn = 0
        cs = cards()
        for c in cs.values():
            for k, allowed in (("phase", PHASES), ("status", STATUSES)):
                if c.get(k) not in allowed:
                    print(f"{c['id']}: {k}={c.get(k)!r}"); bad += 1
            if not re.fullmatch(r"M\d+", c.get("milestone", "")):
                print(f"{c['id']}: milestone={c.get('milestone')!r}"); bad += 1
            if not 1 <= val(c) <= 5:
                print(f"{c['id']}: value={c.get('value')!r}"); bad += 1
            for k, minimum in (("tries", 0), ("try_cap", 1)):
                if k in c and (not re.fullmatch(r"[0-9]+", c[k]) or int(c[k]) < minimum):
                    print(f"{c['id']}: {k}={c[k]!r} (integer >= {minimum} required)"); bad += 1
            for d in lst(c.get("deps")):
                if d not in cs:
                    print(f"{c['id']}: unknown dep {d}"); bad += 1
            if os.path.basename(c["_path"]) != f"{c['id']}.md":
                print(f"{c['id']}: file name {c['_path']}"); bad += 1
            if c.get("status") != "done":  # a path that exists nowhere: a typo, or a new file the entry should mark `(new)`
                br = (c.get("branch") or "").strip()
                for e in lst(c.get("files")):
                    if "(new" not in e.lower():
                        for f in file_paths(e):
                            if not os.path.exists(os.path.join(ROOT, f)) and not (br and subprocess.run(
                                    ["git", "-C", REPO, "cat-file", "-e", f"{br}:{f}"], capture_output=True).returncode == 0):
                                print(f"warn {c['id']}: files: {f} exists nowhere (mark a new file `{f} (new)`)"); warn += 1
        print(f"{len(cs)} cards, {bad} problems, {warn} files: warnings"); sys.exit(1 if bad else 0)
    elif a.cmd == "self-test":
        sys.exit(self_test())
    elif a.cmd == "check-absence":
        bad, rep = check_absence(a.doc, a.base)
        print("\n".join(rep) or f"{a.doc}: no unreceipted absence claims")
        print(f"check-absence: {bad} problem(s)" + (" (warn only)" if a.warn and not a.strict else ""))
        sys.exit(1 if bad and not (a.warn and not a.strict) else 0)
    elif a.cmd == "metrics":
        rows, summ = metrics()
        out = a.out or os.path.join(LOOP, "metrics.csv")
        text = "section,key,value\n" + "".join(f"{s_},{k},{v}\n" for s_, k, v in rows)
        if a.csv:
            sys.stdout.write(text)
        else:
            open(out, "w").write(text)
            print("\n".join(summ) + f"\n({len(rows)} CSV rows -> {os.path.relpath(out, ROOT)})")
    elif a.cmd == "commit-docs":
        rc, msg = commit_docs(a.paths, a.message, os.getpid())
        print(msg); sys.exit(rc)
    elif a.cmd == "finish-attempt":
        finish_attempt(json.loads(os.environ.get("OPENBF_JOB_JSON") or "{}"), a.since)
    elif a.cmd == "postmortem":
        job = json.loads(os.environ.get("OPENBF_JOB_JSON", "{}") or "{}")
        line = postmortem(job, a.since, a.pid, a.lane)
        print(line.rstrip() or "session wrapped up")
    elif a.cmd == "landed":
        cs = cards(); c = cs[a.id]
        c["status"] = "done"; c["resume"] = f"landed {a.sha} {datetime.date.today()}"
        if "tries" in c:  # a landing is a result: a reopened multi-slice card starts its next slice at try 1
            c["tries"] = "0"
        write_card(c)
        s = state()
        s.setdefault("landed_at", {})[a.id] = time.time()
        s["landings_since_planner"] = s.get("landings_since_planner", 0) + 1
        s["landings_since_cleanse"] = s.get("landings_since_cleanse", 0) + 1
        s.setdefault("last_landed_milestone", {})[c.get("milestone", "")] = datetime.datetime.now().isoformat(timespec="seconds")
        save_state(s); write_backlog()
        print(f"{a.id} done ({a.sha})")


if __name__ == "__main__":
    main()
