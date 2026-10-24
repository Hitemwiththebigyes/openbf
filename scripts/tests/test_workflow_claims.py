"""Synthetic claim contention and landing eligibility; no game, gates or loop runs."""
import contextlib
import copy
import io
import multiprocessing
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import land
import loopctl


def contender(locks, start, finish, results, transfer=None):
    loopctl.LOCKS = locks
    start.wait(10)
    if transfer is None:
        won = loopctl.claim("MAIN", os.getpid())
    else:
        won = land.take_main(os.getpid(), {"main_holder": transfer})
    results.put((os.getpid(), won))
    # Each proposed owner remains a live process until every result is collected.
    finish.wait(10)


@unittest.skipUnless(sys.platform.startswith("linux"), "Linux workflow locking")
class ClaimTests(unittest.TestCase):
    def test_process_contention_and_guarded_release(self):
        ctx = multiprocessing.get_context("spawn")
        for kind in ("dead", "pidless", "saved-holder"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as tmp, mock.patch.object(loopctl, "LOCKS", tmp):
                d = Path(tmp) / "MAIN"
                d.mkdir()
                if kind == "pidless":
                    os.utime(d, (time.time() - 120, time.time() - 120))
                else:
                    (d / "pid").write_text(str(os.getpid()) if kind == "saved-holder" else "999999999")
                start, finish, results = ctx.Event(), ctx.Event(), ctx.Queue()
                workers = [ctx.Process(target=contender, args=(tmp, start, finish, results,
                           str(os.getpid()) if kind == "saved-holder" else None)) for _ in range(4)]
                try:
                    for p in workers:
                        p.start()
                    start.set()
                    outcomes = [results.get(timeout=10) for _ in workers]
                    winners = [pid for pid, won in outcomes if won]
                    self.assertEqual(len(winners), 1, outcomes)
                    self.assertEqual(loopctl.holder(str(d)), str(winners[0]))
                    inode = (Path(tmp) / ".claims.lock").stat().st_ino
                    # A delayed release/sweeper from the replaced owner cannot erase a live winner.
                    loopctl.release("MAIN", os.getpid())
                    with mock.patch.object(loopctl, "locks", return_value={"MAIN": "999999999"}):
                        loopctl.sweep_claims(os.getpid())
                    self.assertEqual(loopctl.holder(str(d)), str(winners[0]))
                    self.assertNotIn(".claims.lock", loopctl.locks())
                    loopctl.release("MAIN", winners[0])
                    self.assertFalse(d.exists())
                    self.assertTrue(loopctl.claim("MAIN", os.getpid()))
                    self.assertEqual((Path(tmp) / ".claims.lock").stat().st_ino, inode)
                finally:
                    finish.set()
                    for p in workers:
                        p.join(timeout=10)
                        if p.is_alive():
                            p.terminate()
                            p.join()
                        self.assertEqual(p.exitcode, 0)

    def test_fresh_live_and_stale_sweep(self):
        with tempfile.TemporaryDirectory() as tmp, mock.patch.object(loopctl, "LOCKS", tmp):
            d = Path(tmp) / "MAIN"
            d.mkdir()
            self.assertFalse(loopctl.claim("MAIN", os.getpid()))
            loopctl.sweep_claims("999999999")
            self.assertTrue(d.exists(), "fresh pid-less grace must survive sweeping")
            os.utime(d, (time.time() - 120, time.time() - 120))
            loopctl.sweep_claims("999999999")
            self.assertFalse(d.exists())
            self.assertTrue(loopctl.claim("MAIN", os.getpid()))
            self.assertFalse(loopctl.claim("MAIN", "999999999"))
            self.assertTrue(land.take_main(os.getpid(), {}))
            self.assertFalse(land.take_main("999999999", {}))
            loopctl.sweep_claims(os.getpid())
            self.assertFalse(d.exists(), "the lane's own leftover is swept")


def row(gate, verdict="PASS", value="ok"):
    return dict(gate=gate, verdict=verdict, value=value, exit=0, cmd="fixture", expected="fixture")


class Phase2Tests(unittest.TestCase):
    def fixture(self, tmp):
        out = Path(tmp)
        for name in ("main", "branch"):
            (out / name / "goldens").mkdir(parents=True, exist_ok=True)
            (out / name / "SUMMARY.md").write_text("fixture summary")
            (out / name / "goldens" / "QUANT.md").write_text("fixture quant")
        (out / "gates.toml").write_text("[fidelity]\nenabled = false\n")
        return dict(phase=1, step="done", done=list(land.STEPS1), groups=["ci", "gpu", "map"],
                    rows_branch=[row("ci"), row("gpu"), row("map"), row("protect")], rows_main=[row("ci"), row("gpu")],
                    moved_goldens=[], problems=[], after="tip", worktree=str(out / "absent"), out=tmp)

    def run_phase2(self, tmp, st, declares="", reject=(), resume=False, current=""):
        calls, head = [], [st.get("golden_commit", "tip")]

        def git(*args, **kw):
            if args[0] == "status":
                return current
            if args[:2] == ("rev-parse", "HEAD"):
                return head[0]
            if args[0] == "rev-parse":
                return "tip"
            calls.append(("git", args[0]))
            if args[0] == "commit":
                head[0] = "golden"
            return ""

        with contextlib.ExitStack() as stack:
            for name, value in dict(paths=lambda c: ("fixture", "", "", tmp), load_state=lambda out: st,
                    take_main=lambda pid, state: True, GATES_TOML=str(Path(tmp) / "gates.toml"), git=git,
                    save=lambda state: calls.append(("save", state["step"])),
                    restore_from_head=lambda *a: calls.append(("restore", a[-1])),
                    rebaseline=lambda *a: calls.append(("rebaseline",)),
                    gates=lambda *a, **kw: calls.append(("gates",)) or [row("gpu")],
                    journal=lambda *a: calls.append(("journal",))).items():
                stack.enter_context(mock.patch.object(land, name, value))
            stack.enter_context(mock.patch.object(land.subprocess, "run", lambda *a, **kw: calls.append(("landed",))))
            stack.enter_context(mock.patch.object(loopctl, "release", lambda *a, **kw: calls.append(("release",))))
            stack.enter_context(mock.patch.object(loopctl, "remove_target", lambda *a, **kw: calls.append(("cleanup",)) or True))
            stack.enter_context(contextlib.redirect_stdout(io.StringIO()))
            try:
                land.phase2(dict(id="FIXTURE", declares=declares), list(reject), os.getpid(), resume=resume)
            except SystemExit as exc:
                return calls, str(exc)
        return calls, None

    def test_refuses_all_failures_before_mutation(self):
        for gate in ("ci", "gpu", "sim", "fidelity"):
            for verdict in ("FAIL", land.gates_mod.ADAPTER_LOST):
                with self.subTest(gate=gate, verdict=verdict), tempfile.TemporaryDirectory() as tmp:
                    st = self.fixture(tmp)
                    st["rows_main"].append(row(gate, verdict))
                    calls, error = self.run_phase2(tmp, st, declares="sim;goldens=any", reject=["a.png"])
                    self.assertIn("NOT LANDABLE", error)
                    self.assertEqual(calls, [])

    def test_missing_provenance_and_row_families(self):
        mutations = [lambda st: st.pop("groups"), lambda st: st.update(groups=["gpu"]),
                     lambda st: st.update(main_groups=[]), lambda st: st.pop("rows_branch"),
                     lambda st: st.pop("rows_main"), lambda st: st.pop("moved_goldens"),
                     lambda st: st.update(done=[]), lambda st: st.update(rows_main=[]),
                     lambda st: st["rows_main"].pop(),
                     lambda st: st.update(groups=st["groups"] + ["sim"],
                                         rows_branch=st["rows_branch"] + [row("sim")]),
                     lambda st: st.update(groups=st["groups"] + ["arena"],
                                         rows_branch=st["rows_branch"] + [row("arena_flat")]),
                     lambda st: st.update(groups=st["groups"] + ["leak"],
                                         rows_branch=st["rows_branch"] + [row("leak")]),
                     lambda st: st.update(groups=st["groups"] + ["sim"],
                                         rows_branch=st["rows_branch"] + [row("sim"), row("simhash")],
                                         rows_main=[row("ci"), row("gpu"), row("sim")])]
        for i, change in enumerate(mutations):
            with self.subTest(case=i), tempfile.TemporaryDirectory() as tmp:
                st = self.fixture(tmp)
                change(st)
                calls, error = self.run_phase2(tmp, st)
                self.assertIn("NOT LANDABLE", error)
                self.assertEqual(calls, [])

    def test_declared_moves_count_changes_and_rejected_goldens(self):
        for reject in ([], ["maps/a.png"], ["maps/a.png", "maps/b.png"]):
            with self.subTest(reject=reject), tempfile.TemporaryDirectory() as tmp:
                st = self.fixture(tmp)
                st["groups"] += ["sim", "arena", "leak", "sweep"]
                st["rows_branch"] += [row(g, "MOVED") for g in ("sim", "simhash", "arena_flat", "arena_tat2", "leak", "sweep")]
                st["rows_branch"].append(row("ordnance"))
                st["rows_main"] = copy.deepcopy(st["rows_branch"])
                st["main_groups"] = list(st["groups"])  # main reused the complete branch plan
                st["rows_main"][0] = row("ci", "MOVED", "20 / 0 / 3")
                st["rows_main"][1] = row("gpu", "MOVED")
                # A nonzero MOVED exit is intentionally accepted.
                next(r for r in st["rows_main"] if r["gate"] == "sim")["exit"] = 1
                st["moved_goldens"] = ["engine/goldens/maps/a.png", "engine/goldens/maps/b.png"]
                # Old problem text may be resolved by rejection; saved rows are authoritative.
                st["problems"] = ["stale undeclared golden problem"]
                allowed = [p for p in ("maps/a.png", "maps/b.png") if p not in reject]
                declares = "sim;arena;leak;sweep;goldens=" + ",".join(allowed)
                calls, error = self.run_phase2(tmp, st, declares, reject)
                self.assertIsNone(error)
                self.assertIn(("rebaseline",), calls)
                self.assertIn(("cleanup",), calls)
                self.assertIn(("landed",), calls)
                self.assertEqual(sum(c[0] == "restore" for c in calls), len(reject))
                self.assertEqual(("git", "commit") in calls, bool(allowed))

    def test_map_requires_protect_and_branch_plan_requires_map(self):
        for omitted in ("branch-map", "branch-protect", "main-protect", "none", "approved-move"):
            with self.subTest(omitted=omitted), tempfile.TemporaryDirectory() as tmp:
                st = self.fixture(tmp)
                st["main_groups"] = list(st["groups"])
                st["rows_main"] = copy.deepcopy(st["rows_branch"])
                if omitted == "branch-map":
                    st["groups"].remove("map")
                elif omitted.endswith("protect"):
                    name = omitted.split("-")[0]
                    st["rows_" + name] = [r for r in st["rows_" + name] if r["gate"] != "protect"]
                elif omitted == "approved-move":
                    for name in ("branch", "main"):
                        next(r for r in st["rows_" + name] if r["gate"] == "protect")["verdict"] = "MOVED"
                calls, error = self.run_phase2(tmp, st)
                if omitted in ("none", "approved-move"):
                    self.assertIsNone(error)
                    self.assertIn(("landed",), calls)
                else:
                    self.assertEqual(calls, [])
                    self.assertIn("group plan" if omitted == "branch-map" else "protect", error)

    def test_undeclared_moves_bake_and_new_golden_inventory(self):
        for gate in ("sim", "simhash", "arena_flat", "sweep", "leak", "parity", "bake", "gpu"):
            with self.subTest(gate=gate), tempfile.TemporaryDirectory() as tmp:
                st = self.fixture(tmp)
                st["rows_main"].append(row(gate, "MOVED"))
                if gate == "gpu":
                    st["moved_goldens"] = ["engine/goldens/a.png"]
                calls, error = self.run_phase2(tmp, st)
                self.assertIn("NOT LANDABLE", error)
                self.assertEqual(calls, [])
        with tempfile.TemporaryDirectory() as tmp:
            st = self.fixture(tmp)
            calls, error = self.run_phase2(tmp, st, "goldens=any", current=" M engine/goldens/new.png")
            self.assertIn("missing from phase 1 inventory", error)
            self.assertEqual(calls, [])

    def test_clean_resume_and_enabled_fidelity_completeness(self):
        with tempfile.TemporaryDirectory() as tmp:
            st = self.fixture(tmp)
            calls, error = self.run_phase2(tmp, st)
            self.assertIsNone(error)
            self.assertIn(("rebaseline",), calls)
            # Resuming past golden and baseline steps does not replay those mutations.
            st = self.fixture(tmp)
            st.update(phase=2, step="gpu_after", done2=["goldens", "toml"], reject=[])
            calls, error = self.run_phase2(tmp, st, resume=True)
            self.assertIsNone(error)
            self.assertNotIn(("rebaseline",), calls)
            self.assertIn(("landed",), calls)
            st = self.fixture(tmp)
            (Path(tmp) / "gates.toml").write_text("[fidelity]\nenabled = true\n")
            st["groups"].append("sweep")
            st["rows_branch"].append(row("sweep"))
            st["rows_main"].append(row("sweep"))
            calls, error = self.run_phase2(tmp, st)
            self.assertIn("fidelity", error)
            self.assertEqual(calls, [])
            st["rows_branch"].append(row("fidelity"))
            st["rows_main"].append(row("fidelity"))
            calls, error = self.run_phase2(tmp, st)
            self.assertIsNone(error)
            self.assertIn(("landed",), calls)


class RankingTests(unittest.TestCase):
    def test_exact_previous_order(self):
        cards = {cid: dict(id=cid, value=str(value), first=first, milestone=milestone, status=status, alpha=alpha)
                 for cid, value, first, milestone, status, alpha in
                 [("Z", 5, "", "M1", "todo", "yes"), ("A", 5, "", "M1", "todo", "yes"),
                  ("NEG", 1, "-1", "M2", "todo", ""), ("ZERO", 1, "0", "M1", "todo", ""),
                  ("D", 9, "-10", "M1", "done", ""), ("B", 5, "", "M2", "todo", "yes") ]}
        last = {"M1": "2026-10-22", "M2": "2026-10-21"}
        order = list(cards)
        before = sorted([c for c in cards.values() if c["status"] != "done"], key=lambda c:
                (loopctl.first_rank(c) is None, loopctl.first_rank(c) or 0, -loopctl.val(c), not loopctl.is_alpha(c),
                 last.get(c.get("milestone", ""), ""), order.index(c["id"])))
        with mock.patch.object(loopctl, "state", return_value={"last_landed_milestone": last}):
            self.assertEqual(loopctl.ranked(cards), before)


if __name__ == "__main__":
    unittest.main()
