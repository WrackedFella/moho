
Verdict: reject
Route: human
Sign-off: human
Head: dca46335b59f60998d43915d23cc8f8496f1cff3   Tier: full   Round: 1

Blockers
- [X1] B4, flagstone/src/app/game.rs:377-394. Fix: needs-decision.
  Failure scenario: in strategy (Isometric) mode, `render_scene` runs once per frame. Each time, it builds a fresh `with_body: Vec<Cube>` by copying every actor cube and adding the pawn cube. The allocation and copy cost grow with the actor count, every frame.
  Evidence (rule): `rust-standards/SKILL.md`, Resources and memory: "Hot paths: no per-frame allocation or needless dynamic dispatch in inner loops; reuse buffers. Measure before optimizing, but don't design allocation in." No ADR, `CLAUDE.md` rule or `_todo/_STANDARDS.md` entry waives it. Only the work item accepts it (Risks: "Transient cube per frame: one small allocation in `render_scene`"), and that contradicts the written rule, so the fix needs a decision.
  Expected fix: keep a reusable `Vec<Cube>` on `App` that is cleared and refilled each frame, or send the pawn cube to the renderer as a separate one-instance draw. If the allocation stays, record a decision that waives the rule here.

Questions
- none

Follow-ups
- Not filed (dry run): [B1, needs-decision] flagstone/src/app/game.rs:209-219. When the strategy view-height cut is active, a pawn drawn below the cut cannot be selected. The click lands on terrain above the cut, which the renderer has hidden, because `raycast_surface` ignores the cut. Setup: pawn top at y=6.3, roof at y=10-11, view level 8 (cut 8.0039). The click hits the hidden roof (y=12, distance 3.27) and selection stays None. Reproduced by the refuter with a scratch test in `mod tests`, run with `cargo nextest run -p flagstone --all-features -E 'test(scratch_pawn_visible_under_a_view_cut_is_selectable)'`. The reverse also holds: a pawn above the cut is not drawn but can still be clicked. Mining marks (`apply_mark`) are affected the same way. Suggested acceptance criterion: with the view-height cut active, a click picks only pawns and surfaces that are drawn. Needs a scope decision, because the card lists "Changing `raycast_surface`" as out of scope.

Sign-off triggers
- 1158 changed lines, over 400 excluding lockfiles

Dropped
- none

Not reviewed
- Three-OS CI matrix (`gh workflow run CI`) was not run, and CI state on the head SHA is none (dry-run range).
- Manual Verification checks (mark legibility, audio cues) need a GPU and audio; not run.
- The design critic could not list open child items of SG-F5 (#309) and SG-F4 (#310). Its C3 collision check relied on `wiki/roadmap.md` and `wiki/flagstone/roadmap.md` only.
- The defect finder's `just check` comment-refs step ran against `origin/dev`, the justfile default, not the base SHA.

Counts
- candidates 2, dropped 0, blockers 1, follow-ups 1, agent calls 4 (defect-finder, design-critic, refuter x2), design-critic model: default (inherited)

