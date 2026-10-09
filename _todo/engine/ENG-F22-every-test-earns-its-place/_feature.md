# ENG-F22 — Every test can fail and earns its place

**Issue:** [#167](https://github.com/WrackedFella/moho/issues/167)
**Status:** Backlog
**Labels:** feature, line:engine

## End state

Every test in the workspace can fail for a reason someone would care about, and each behaviour the code promises is pinned by one test at the right level. A failing test then means a broken behaviour, and a green run means something.

## Summary

The 2026-10 test-critic audit judged 498 existing tests (252 keep, 110 strengthen, 44 merge, 92 remove) and found 56 coverage gaps. Each verdict was then checked against `dev` and corrected where the audit was wrong. This feature applies the verified verdicts: it deletes tests that can't fail or duplicate a twin, folds same-scenario tests into tables, tightens tests that a wrong implementation passes, adds the missing tests, and fixes the defects those tests expose.
**Moves toward the end state by:** delivering ENG-F9's "no tautological or vacuous tests" and "tests for code with no production caller are deleted with that code" exit criteria for the audited files, ahead of the mutation baseline.

## Exit criteria

- Every test the verified audit marks remove or merge is gone or folded, except those a filed card already deletes or rewrites (listed per card).
- Every strengthen item and gap in the cards is pinned by a test that was shown red against the wrong implementation it targets.
- No test reads or writes `moho_ui/config/prefs.ini` or another cwd-relative file it doesn't own.
- The confirmed defects (keybind conflict confirm, sprint conflict, chunk streaming at negative coordinates, mixed-chunk meshing, thin-feature normals) are fixed, each with a test that failed before the fix.
- `just check` passes; no lint is silenced and no test is skipped or ignored to get there.

## Scope

- In: tests in every workspace crate and the binary; deleting small production items whose only caller is a test that this feature removes; seams named in a card's tech spec; fixes for the defects above.
- In, by decision 2026-10-08: deleting the uncalled voxel subsystems; the small UI and game behaviours settled in #186 (ENG-F22-19) and #187 (ENG-F22-20).
- Out: `LightSystem`'s event path, the pause path, `input_handling` (listed under Deferred); coverage targets; the `just mutants-full` baseline (ENG-F9).

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Removal and merge work vs new tests | Separate cards per area | Removal is mechanical; new tests need hand-computed values and judgment. Splitting lets each run on the model that fits it |
| Tests of code a filed card deletes or rewrites | Left alone here | #149 (ENG-F12-02), #138 (ENG-F10-01), #145 (ENG-F11-03), #156 (ENG-F20-02) and #141 (ENG-F10-04) own those files; editing them twice is waste |
| A test that pins current behaviour passes on arrival; what is "red"? | Each new or tightened test is run once against the wrong implementation its row names (a temporary local edit, reverted) and the PR records the failure | Orchestrate's red step assumes a failing test; for test-only work, the named mutation is the red |
| How is a removal shown safe? | `cargo mutants --file` on each source file whose tests a card deletes, on `dev` and on the branch; the branch adds no missed mutant, or the PR names each new one and why it is acceptable | The audit built nothing; this proves no removed test was the only one catching something |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| Wire `LightSystem` to world events and fix relight after an opaque block is removed | No material emits light in production, so block light is always zero and wiring changes nothing visible | The first light-emitting block (torch, lava) is planned |
| Pause/resume: the path, its bugs and the coordinator "valid from" docs | Paused is unreachable today and pause needs its own look | The user schedules pause work |
| Numeric key codes in prefs, modifier bits dropped on save | Only a hand-edited file reaches the first; #149 (ENG-F12-02) replaces the binding format | #149 |

## Items

| Item |
|---|
| #168 [ENG-F22-01](https://github.com/WrackedFella/moho/issues/168): binary, prefs, events and state tests carry no duplicates or can't-fail tests |
| #169 [ENG-F22-02](https://github.com/WrackedFella/moho/issues/169): voxel and lighting tests carry no duplicates or tests of uncalled code |
| #170 [ENG-F22-03](https://github.com/WrackedFella/moho/issues/170): game, audio and renderer tests carry no duplicates or tests of uncalled code |
| #171 [ENG-F22-04](ENG-F22-04-ui-tests-carry-no-duplicates-placeholders.md): UI tests carry no duplicates, placeholders or tests of uncalled code |
| #172 [ENG-F22-05](ENG-F22-05-binary-input-camera-and-day-night-tests.md): binary input, camera and day-night tests fail for wrong implementations |
| #173 [ENG-F22-06](ENG-F22-06-voxel-grid-and-meshing-tests.md): voxel grid and meshing tests fail for wrong implementations |
| #174 [ENG-F22-07](ENG-F22-07-lighting-tests-fail-for-wrong-implementations.md): lighting tests fail for wrong implementations |
| #175 [ENG-F22-08](ENG-F22-08-event-bus-input-and-state-tests-pin.md): event bus, input and state tests pin their contracts |
| #176 [ENG-F22-09](ENG-F22-09-game-rule-tests-fail-for-wrong-implementations.md): game rule tests fail for wrong implementations |
| #177 [ENG-F22-10](ENG-F22-10-physics-and-audio-tests.md): physics and audio tests fail for wrong implementations |
| #178 [ENG-F22-11](ENG-F22-11-renderer-tests-fail-for-wrong-implementations.md): renderer tests fail for wrong implementations and every loaded shader is validated |
| #179 [ENG-F22-12](ENG-F22-12-settings-tests-stay-off-the-real-prefs-file.md): settings tests stay off the real prefs file and pin every binding |
| #180 [ENG-F22-13](https://github.com/WrackedFella/moho/issues/180): HUD, console and UI routing tests fail for wrong implementations |
| #181 [ENG-F22-14](https://github.com/WrackedFella/moho/issues/181): confirming a keybind conflict moves the key, Sprint included |
| #182 [ENG-F22-15](https://github.com/WrackedFella/moho/issues/182): chunk streaming centres on the player's chunk at negative coordinates |
| #183 [ENG-F22-16](https://github.com/WrackedFella/moho/issues/183): mixed smooth and blocky chunks mesh with valid indices and aligned geometry |
| #184 [ENG-F22-17](https://github.com/WrackedFella/moho/issues/184): lone smooth voxels have outward normals |
| #185 [ENG-F22-18](https://github.com/WrackedFella/moho/issues/185): voxel code nothing calls is gone |
| #186 [ENG-F22-19](ENG-F22-19-compass-debug-view-world-name-menu-music.md): compass, debug view, world name, menu music and volume sliders behave as decided |
| #187 [ENG-F22-20](ENG-F22-20-hotbar-slot-range-config-defaults-and-missing.md): hotbar slot range, config defaults and missing-character moves behave as decided |

## Notes

- Land the moho_core voxel and lighting cards before #140 (ENG-F10-03) branches, or after it merges: that card moves those files byte-for-byte and checks the test count.
- Overlaps drafts ENG-F1-04 (keybind test tiers) and ENG-F1-07 (placeholder `#[ignore]` tests); those deliverables can be dropped once the matching cards here merge.
- Model per card: removal and merge cards suit Sonnet; test-writing and bug cards suit Opus.
