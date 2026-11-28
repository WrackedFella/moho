
Verdict: reject
Route: fix
Sign-off: human
Head: e377e53d2ac1e623f60db5539dc7255cba1dac82   Tier: full   Round: 1

Blockers
[X1] B2, searchlight/src/sim.rs:811-815. Failure scenario: the criterion "A faster round arrives sooner" (two round definitions that differ only in muzzle velocity) is met only by `the_faster_of_two_rounds_hits_a_dummy_60_m_away_first` (sim.rs:2062). That test builds a separate sim per speed, and each sim's rounds table holds one entry (`torso_config_with_round`, sim.rs:1940). So the `Shot::Fired(ItemId)` → `rounds` lookup in `launch_round` is never tested, and a kit that names two rounds (which `KitDef::resolve` supports) could fly the wrong round's ballistics without failing anything. Evidence: the refuter changed `launch_round` to ignore the fired round (`let _ = round; let Some(ballistics) = self.origin.1.rounds.values().next().copied() else {`) and ran `cargo nextest run -p searchlight -p searchlight_rules --no-fail-fast`: 779 passed, 0 failed. Expected fix: add a sim test with one magazine holding two rounds of different muzzle velocity, and check that each pull lands on the tick its own ballistics predict.
[X2] B4, searchlight_rules/src/ballistics.rs:63-69 (exported at searchlight_rules/src/lib.rs:19-20). Failure scenario: `GravityScaleError` is a public `#[non_exhaustive]` struct with a single `value: f32` field. The three refusals (negative, non-finite, above `MAX_GRAVITY_SCALE`) all collapse into it, so a caller can't branch on which one happened. Rule: devflow:rust-standards, Error contracts at crate boundaries: "Define the public error as a `thiserror` enum." Also engineering-standards: "A library's public error type is its contract; callers can branch on it." `UnknownGrade` (grade.rs:18-20) is older code this diff doesn't touch, so it doesn't make the struct allowed. Expected fix: make `GravityScaleError` a `thiserror` enum, e.g. `Negative { value }`, `NonFinite { value }`, `AboveMax { value }`, and keep `#[non_exhaustive]`.

Questions
None.

Follow-ups
Not filed (dry run):
1. B1 (follow-up, Fix: needs-decision), searchlight/src/sim.rs:839-845, and wiki/searchlight/architecture.md:160. A round that hits an enemy can also count a near miss on that same enemy. The struck enemy is left out only on the segment where the strike happens, so the segment before it can pass within `near_miss_radius` and count. Repro: a Novice enemy 30.75–33 m straight ahead, `near_miss_radius` 1.5, one level round at the torso. 11 of 160 direct hits also counted `near_misses == 1` (scratch test, rerun by the refuter). Simple (hitscan) never does this. The code matches the tech spec ("every living enemy but the one this segment struck"), but the wiki text in this diff says "except the enemy the round hit". Choose one of these, then make the code and the wiki agree: hold the count back until the round lands (this conflicts with "put under fire on the tick the round passes it"), or reword the wiki to say the enemy struck on that segment. Suggested acceptance criterion: either a test that a direct hit never adds a near miss on the struck enemy, or wiki text that matches the per-segment rule.
2. Test coverage, searchlight/src/sim.rs:847. No test checks that a round's hit reports `round.from` (where the player stood when firing) and not the player's position when the round lands. Evidence: passing `self.player().position` instead of `round.from` to `apply_player_hit` left 624 of 624 searchlight tests passing (rerun by the refuter). This is a tech-spec design item that the code already delivers. No acceptance criterion covers it, so it is not B2. Suggested acceptance criterion: the player fires a non-lethal round at an enemy and strafes before it lands; the test asserts the enemy's `under_fire` equals the firing position.

Sign-off triggers
- dependencies or licenses changed: Cargo.lock, searchlight_rules/Cargo.toml
- 2730 changed lines, over 400 excluding lockfiles

Dropped
None. Every candidate was confirmed. The design-critic's question on `GravityScaleError` was settled as the B4 blocker X2. The defect-finder's B1 was confirmed as a follow-up, not a blocker: the code follows the tech spec, the lasting effect is one extra near miss, and either fix is cheap to undo.

Not reviewed
- CI on the head SHA: none (dry run over a commit range).
- Reverting production code while keeping the tests: the criteria tests live in `#[cfg(test)]` modules inside the production files, so targeted code changes were used instead.
- `searchlight/src/sim.rs` and `round.rs` are outside the mutation scope; they were checked by hand-made code changes only.
- The design-critic matched the ~700 added `sim.rs` tests to the test map by name only.
- The work item's Footprint names ADR `0031-rounds-fly-as-stepped-segments.md`; that ADR landed on dev as 0033. The design-critic checked the diff against 0033.
- `just check` passed at the head (fmt, clippy, nextest 3379/3379, doctests, comment refs; comment refs ran against origin/dev, not the base). `just mutants` against the base: 55 mutants, 48 caught, 7 unviable, 0 survivors.

Counts
candidates 4, dropped 0, blockers 2, follow-ups 2, agent calls 6 (2 finders, 4 refuters), design-critic model: agent default (no override)

