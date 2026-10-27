# Contributing to openbf

Bug reports, feature ideas, playtesting, documentation and code are welcome.
Start with the [open issues](https://github.com/sprucey149/openbf/issues) before
writing code. Issues are how contributors and the maintainer's automated agents
agree on who is doing what.

## Choose and claim a task

1. Search existing issues, including related features and bugs. Browse
   [unassigned issues](https://github.com/sprucey149/openbf/issues?q=is%3Aissue%20is%3Aopen%20no%3Aassignee)
   for available proposals. An assigned issue
   is reserved; do not implement overlapping work without coordination.
2. For an unassigned issue, comment that you would like to take it and outline
   your approach. Wait for the maintainer to confirm the scope and assign it to
   you before coding. Unassigned does not mean every prerequisite is ready.
3. If your idea has no issue, open one describing what players would see, the
   intended result and the proposed scope. Agree on it before implementation.
4. Read the linked specification, `AGENTS.md`, `PLAN.md` and the relevant
   `engine/CRATES.md` entries. Point out missing inputs or unresolved decisions
   before expanding the scope.

The cards in `docs/loop/cards/` contain detailed specifications and implementation
slices. Not every card needs its own public issue. A card without an issue is
not automatically free to take; propose an issue and confirm ownership first.
Discord can help with discussion, but record the agreed scope and ownership on
the issue so everyone and their agents can see it.

If you pause work, change direction or discover overlap, leave a short update on
the issue. Claim the next task separately; merging one PR does not reserve another.

## Build and check your change

Fork the repository and create a feature branch from current `main`. Follow the
[README build instructions](README.md#building-your-copy) to prepare your own game
data, then run the relevant focused checks and the normal integration checks:

```bash
cd engine
cargo build --release
./ci.sh          # fmt, clippy -D warnings, build, tests and doctests
```

Some checks require baked maps, missions, fonts, audio or graphics hardware.
Report which checks actually ran, their results, and any failed, skipped or
ignored cases. A skipped data-dependent test does not verify the behavior.
Fixture tests are useful for logic; they do not replace required real-map,
rendering, playback or original-game evidence. Say which checks remain unrun.

For a bug fix, retain a regression that fails for the original defect and passes
with the fix. For UI or rendering work, include matched before/after screenshots
or a short demonstration. Describe the commands, build revision and required
inputs so someone else can reproduce the result.

## Keep pull requests focused

- Use one feature or coherent bug fix per branch and PR. Agree on smaller slices
  for a large feature; avoid combining unrelated fixes, refactors and generated
  changes into one large review.
- Open a draft PR early for substantial work, especially when shared input,
  profile, rendering or AI code is involved.
- Link the issue and explain the before/after behavior, scope, evidence and
  remaining checks. Use `Closes #NUMBER` only when the whole issue is complete;
  use `Refs #NUMBER` for a partial slice.
- Fetch current upstream `main` before submitting or updating the PR. Preserve
  other contributors' work and explain any integration conflict.

Maintainers coordinate integration into upstream `main`. Contributors use their
own branches and PRs rather than independently publishing to `main`. Issue
assignment and agreed source scope apply equally to human and AI-assisted work.

## AI-assisted contributions

AI assistance is welcome. Have your agent read `AGENTS.md`, the issue and related
specifications, and respect existing assignments before selecting work. Review
the complete generated diff yourself before submitting it. You remain responsible
for its correctness, clarity and evidence; a successful agent run is not a test
result. Do not claim unrun checks passed or silently broaden the feature.

## Project rules

- **Keep game content out of Git and PR attachments:** no textures, models,
  sounds, disc images, extracted scripts, save states or packed game executables.
  Players build content from their own copies. Share source, commands and permitted
  diagnostic evidence instead.
- **Keep the simulation deterministic:** no wall-clock dependencies, OS randomness
  or host floating-point library calls in `engine/crates/core`; use the project's
  established deterministic helpers, including `core::fmath`.
- **Support original-game behavior with evidence:** cite the relevant `analysis/`
  note and primary references. Mark guesses as `PROVISIONAL`; discuss intentional
  gameplay differences before implementing them.
- **Preserve validation standards:** do not change gates, thresholds, goldens or
  baselines merely to make a check pass. Explain any intended render/bake change
  and agree on its validation before updating references.
- **Follow the module map and current size rules:** keep responsibilities clear,
  update `engine/CRATES.md` for source changes, and use the repository's size/map
  checks. Avoid unrelated formatting or mechanical churn.

## Report bugs and suggest features

For a bug, include the build commit, map/mode, reproduction steps, expected and
actual behavior, and a screenshot or short clip when useful. Press F9 shortly
after a gameplay bug and include the diagnostic snapshot described in the README;
do not attach game assets or a packed build.

For a feature, explain the player experience and what would count as complete.
Search existing issues first and link related cards if you find them. Opening an
issue proposes work; confirmed assignment reserves it.

## License

By contributing, you agree that your contribution is released under the MIT
license (`LICENSE`).
