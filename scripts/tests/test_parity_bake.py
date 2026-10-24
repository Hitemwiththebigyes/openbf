"""PARB: real lock exclusion and synthetic cache invalidation, without GPU/assets."""
import contextlib
import csv
import io
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import parity


class BakeCache(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.baked = self.root / 'baked'
        self.manifest = self.baked / 'map/manifest.ron'
        self.manifest.parent.mkdir(parents=True)
        self.manifest.write_text('bake A')
        source = self.root / 'engine/crates/game/examples/pose_render.rs'
        source.parent.mkdir(parents=True)
        source.touch()
        for name, value in dict(BAKED=str(self.baked), PUB=str(self.root / 'pub'),
                                ART=str(self.root), LOCK=str(self.root / 'ci.lock')).items():
            self.enterContext(patch.object(parity, name, value))
        self.enterContext(patch.object(parity, 'main_root', return_value=str(self.root)))
        self.enterContext(patch.object(parity, 'git_head', return_value='head'))
        self.enterContext(contextlib.redirect_stdout(io.StringIO()))

    def write_scores(self, out, value):
        out = Path(out)
        out.mkdir(parents=True, exist_ok=True)
        with (out / 'scores.csv').open('w') as f:
            w = csv.DictWriter(f, parity.COLS)
            w.writeheader()
            w.writerow(dict(map='map', spot='spot', block=value, dluma=0, dp99=0))
        return [], {'map': dict(n=1, block=value)}, value, 'head'

    def test_changed_bake_rescores_same_sha_and_ignores_legacy_cache(self):
        legacy = Path(parity.PUB) / 'main/head.csv'
        legacy.parent.mkdir(parents=True)
        legacy.write_text('legacy score must not be reused')
        def run(tree, target, out, **kwargs):
            return self.write_scores(out, 1 if self.manifest.read_text() == 'bake A' else 9)
        with patch.object(parity, 'run_tree', side_effect=run) as render:
            first = parity.main_scores()
            self.assertEqual(parity.main_scores(), first)
            self.assertEqual(render.call_count, 1)
            self.manifest.write_text('bake B')
            second = parity.main_scores()
            self.assertNotEqual(first, second)
            self.assertEqual(render.call_count, 2)
            self.assertEqual(parity.main_scores(), second)
            self.assertEqual(render.call_count, 2)
            self.assertEqual(parity.read_csv(first)[0]['block'], 1)
            self.assertEqual(parity.read_csv(second)[0]['block'], 9)
            self.assertEqual(legacy.read_text(), 'legacy score must not be reused')

    def test_fingerprint_tracks_contents_paths_and_inventory(self):
        first = parity.bake_fingerprint()
        os.utime(self.manifest, None)
        self.assertEqual(first, parity.bake_fingerprint())
        # A common/side/classes bake writes no manifest.ron; its files still count.
        extra = self.baked / 'common/common.ron'
        extra.parent.mkdir()
        extra.write_text('common')
        added = parity.bake_fingerprint()
        self.assertNotEqual(first, added)
        extra.write_text('commoN')
        self.assertNotEqual(added, parity.bake_fingerprint())
        extra.write_text('common')
        self.assertEqual(added, parity.bake_fingerprint())
        extra.rename(extra.parent / 'other.ron')
        self.assertNotEqual(added, parity.bake_fingerprint())
        (extra.parent / 'other.ron').unlink()
        self.assertEqual(first, parity.bake_fingerprint())
        moved = self.baked / 'map2/manifest.ron'
        moved.parent.mkdir()
        self.manifest.rename(moved)
        self.assertNotEqual(first, parity.bake_fingerprint())
        moved.rename(extra.parent / 'other.ron')
        with self.assertRaises(SystemExit):
            parity.bake_fingerprint()

    def test_rebake_waits_across_tree_and_main_and_publication(self):
        # The writer is a separate process with a distinct open file description.
        # Nonblocking probes catch even a missing outer lock deterministically.
        probe = 'import fcntl,sys; f=open(sys.argv[1],"a"); fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)'
        def assert_locked():
            r = subprocess.run([sys.executable, '-c', probe, parity.LOCK], capture_output=True, timeout=5)
            self.assertNotEqual(r.returncode, 0, r.stderr)
            self.assertIn(b'BlockingIOError', r.stderr)
        calls = []
        def run(tree, target, out, **kwargs):
            assert_locked()
            calls.append(tree)
            return self.write_scores(out, 2)
        original_publish = parity.publish
        def publish(*args):
            assert_locked()
            return original_publish(*args)
        args = types.SimpleNamespace(out=str(self.root / 'gate'), allow=None, tree='candidate',
                                     target=None, base=None, threshold=.5)
        with patch.object(parity, 'run_tree', side_effect=run), patch.object(parity, 'publish', side_effect=publish):
            with self.assertRaises(SystemExit) as result:
                parity.cmd_gate(args)
            self.assertEqual(result.exception.code, 0)
        self.assertEqual(calls, ['candidate', str(self.root)])
        # Both sites must share one hold, not unlock/relock between renders.
        # Inspect lock acquisition count as well as the OS-level exclusion above.
        with patch.object(parity, 'ci_lock', wraps=parity.ci_lock) as lock:
            with patch.object(parity, 'run_tree', side_effect=run):
                with self.assertRaises(SystemExit):
                    parity.cmd_gate(args)
            self.assertEqual(lock.call_count, 1)
        subprocess.run([sys.executable, '-c', probe, parity.LOCK], check=True, timeout=5)

    def test_renderer_does_not_reacquire_ci_lock(self):
        target = self.root / 'target'
        exe = target / 'release/examples/pose_render'
        exe.parent.mkdir(parents=True)
        exe.write_text('#!/bin/sh\nexit 0\n')
        exe.chmod(0o755)
        pose = self.root / 'pose.json'
        pose.write_text('{"eye":[0,0,0],"yaw_deg":0,"pitch_deg":0,"fov_v_deg":70}')
        out = self.root / 'render-out'
        original_run = subprocess.run
        def run(argv, **kwargs):
            if argv[0] == 'cargo':
                # Building must be serialized, too.
                with open(parity.LOCK, 'a') as f:
                    import fcntl
                    with self.assertRaises(BlockingIOError):
                        fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)
                return types.SimpleNamespace(returncode=0)
            return original_run(argv, timeout=5, **kwargs)
        with patch.object(parity, 'GPU_LOCK', str(self.root / 'gpu.lock')), patch.object(parity.subprocess, 'run', side_effect=run):
            with parity.ci_lock():
                parity.render(str(self.root), str(target), str(out), [('map', 'spot', str(pose), '')], str(self.baked))


if __name__ == '__main__':
    unittest.main()
