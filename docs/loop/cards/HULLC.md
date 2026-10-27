---
id: HULLC
title: Vehicle hulls cost 37.6 % of the sim: CollisionWorld::set_hull rebuilds every hull part's Body each tick and Body::with_layers re-runs watertight(mesh) - a SipHash weld of the whole mesh - though a part's mesh never changes; compute closed once where the shape is made, then the rest of the flat profile (parry rays 7 %, height_at 2 %, SipHash in core)
milestone: M0
value: 5
phase: build
status: done
deps:
files: engine/crates/core/src/collision/shape.rs, engine/crates/core/src/collision/world.rs
declares:
needs:
effort:
branch: hullc
resume: step 2 landed 8882e856 2026-10-21 (terrain march probe; sim x0.84-0.95); left for the planner: parry AABB ray casts 6-8 %, Barrier::entry_t 7.3 % NAB2, level load 3-5x the 1500-tick sim
created: 2026-10-21
alpha: yes
first: 3
---

## Why
The maintainer's profile (INBOX 2026-10-21; BF2 KAS2, 16 v 16, main of 10-20, callgrind 1,500 ticks): `CollisionWorld::set_hull`
is 37.6 % of all sim instructions (32,641 calls = every vehicle hull, every tick). `set_hull` (`core/src/collision/world.rs:447`)
builds each part with `Body::with_layers(pose * p.local, p.shape.clone(), p.layers)`, and `with_layers`
(`collision/shape.rs:492`) runs `watertight(mesh)` (`shape.rs:513`: two std `HashMap`s, a millimetre vertex weld and an
edge count over the whole mesh) every time - a hull part's mesh never changes. perf's flat profile agrees: SipHash
`hash_one<(u32,u32)>` 13 %, `with_layers` 12 %, rehash 5 %, `[i64;3]` hash 2 %. (Planner checked the call chain on main
d79666c, 2026-10-21; also check what `p.shape.clone()` copies - a mesh by value would be a second per-tick cost.)

## Do
1. Compute `closed` once where the shape is made (the hull part / baked mesh): a `Body::with_layers_closed(.., closed)`, or
   `closed` carried on the shape; `set_hull` and `move_body` (doors, every move) reuse it. Avoid per-tick mesh clones.
2. Then the rest of the flat list, in order, each measured before and after: parry ray casts 7 %, `height_at` 2 %, std
   SipHash anywhere in `core` (a fixed fast hasher; determinism: no result may depend on a map's iteration order).
   Root causes first, threads last (INBOX standing note, 10-20).

## Tools (the maintainer, 10-21)
`perf record -F 499 -o <file> -- <bin>` + `perf report --no-children --sort symbol --stdio` (flat; `--call-graph dwarf`
FAILS here); callers by `valgrind --tool=callgrind` (~50x slower) + `callgrind_annotate --tree=caller --inclusive=yes`.
Copy the binary first, run under the memory cap + `timeout`; wall clock is void under the loop's load - use instruction
counts and shares.

## Acceptance
- simhash and every sim / sweep / arena row unchanged (behaviour-neutral: `declares:` stays empty).
- Before / after pasted for BF2 KAS2, BF1 NAB2 and GEO1 (16 v 16, 1,500 ticks): callgrind total sim instructions and
  `set_hull`'s inclusive share (target: set_hull under 3 %), plus sim ms / tick on a quiet machine (load < 8) or interleaved A B A B.
- A unit test: a hull part's `closed` equals `watertight(mesh)` of the same mesh (built once), mutation-checked.
