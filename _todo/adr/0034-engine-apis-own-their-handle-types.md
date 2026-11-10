# 0034 — Engine APIs own their handle types

**Status:** Proposed; accepted on merge of #712

## Context

The quality baseline found backend types in engine public APIs: wgpu types across
`moho_renderer`, and rapier's `RigidBodyHandle`, `ColliderHandle`, `RigidBodySet` and
`ColliderSet` in `moho_physics` (11 items, plus two `pub use rapier3d::…` re-exports). No
game needs them: flagstone holds body handles for its props and reads set sizes in tests,
searchlight uses none. Each leak makes the once-per-milestone backend upgrade reach the games,
and ties game code to ids the engine does not define, against
[ADR-0011](0011-simulation-stays-network-ready.md). ENG-F44-02 (#678) closed the renderer
side by visibility; [ADR-0027](0027-characters-slide-on-our-own-collide-and-slide.md) keeps
rapier as the physics backend.

## Decision

1. A public engine API names no third-party type. A backend object a game must refer to is
   reached through an engine-defined id: an opaque newtype whose inner field is `pub(crate)`
   (`CharacterHandle`, `LooseId`, and in physics `BodyId` and `ColliderId`). Backend stores are
   private; the engine answers queries about them in engine types.
2. Exceptions, each already decided elsewhere: winit in the platform-adapter crates and
   `Renderer::new` ([ADR-0008](0008-keep-winit-for-windowing-and-input.md)), egui in
   `moho_ui_shell` ([ADR-0017](0017-ui-shell-and-debug-values-are-engine-seams.md)), glam as
   the shared math vocabulary, serde derives, and the shader-validation surface of
   `moho_renderer::pipeline` that its tests read.
3. A test that needs to set up or read backend state outside its crate does so through
   functions behind the crate's `test-support` cargo feature. They follow rule 1 (engine types
   only), and only `[dev-dependencies]` enable the feature.

## Consequences

- A rapier or wgpu upgrade edits only `moho_physics` or `moho_renderer`, games' tests
  included.
- A wrapper id is world-local and not serialisable. Anything that must survive a replay,
  snapshot or the wire gets an engine-issued id, as `LooseId` is.
- A new third-party type in a public signature is a review finding. The rustdoc-JSON check
  that would fail the build stays deferred in ENG-F44 (#661) until a leak recurs.
