# 0035 — Clip playback is simulation state, and a pose is derived from it

**Status:** Accepted on merge of #655 (ENG-F39-05); answers the open questions of engine request [#543](https://github.com/WrackedFella/moho/issues/543)

## Context

[ENG-F39](https://github.com/WrackedFella/moho/issues/635) lets a game play clips on a rigged
model ([#654](https://github.com/WrackedFella/moho/issues/654)) and draw it skinned
([ADR-0030](0030-skinned-meshes-are-skinned-once-per-frame.md)). #655 makes hit volumes and
held items follow bones, so rules now read bone poses.
[ADR-0011](0011-simulation-stays-network-ready.md) requires the tick to own every value a rule
reads and presentation to only read. #543 left open whether animation state is sim data or
render data, and which bone names hit volumes, clips and the first-person body key off.

## Decision

1. **Clip playback is simulation state.** A character's `ClipPlayer` (clip index, clip time,
   speed, loop or once) lives with the game's simulation state and is advanced only by the
   fixed tick, by the tick length. Presentation never advances it.
2. **A pose is derived, never stored.** A pose is a pure function of the model and the
   playback state (`ClipPlayer::pose`). The simulation computes it when a rule needs a bone
   (hit volumes, a bone a rule reads) and does not keep it as state.
3. **Presentation draws the same function.** The frame poses the character from the
   simulation's playback state with the same call, so an uninterpolated frame draws exactly the
   pose the tick read. Presentation may later pose at a clip time between the last two ticks for
   smoothness; that pose is presentation only and never reaches a rule.
4. **Humanoid bones use the Unreal mannequin names.** A model used as a humanoid has nodes
   named `pelvis`, `spine_01`, `neck_01`, `head`, `upperarm_l`, `lowerarm_l`, `hand_l`,
   `thigh_l`, `calf_l`, `foot_l` and their `_r` twins. Other nodes are allowed and ignored. A
   model missing a required name is refused, naming each missing bone.

## Consequences

- A snapshot or replay carries the playback state (a few numbers per character), never bone
  transforms; a pose is recomputed wherever it is needed.
- Poses repeat on the same build and inputs (level L0/L1 of the determinism research); bit
  equality across machines is the netcode ADR's call.
- Blends, layers and sim-timed clips (ENG-F39 slice 4) extend the playback state, not the
  pose: the rule of one pure function from state to pose holds.
- The cost of a sim-read pose is one pose per character per tick that needs it, on the tick
  thread; tens of bones per character, no allocation-free path until a profile asks for one.
- Rigs named another way (Mixamo, VRM) need renaming at export; name maps and retargeting are
  asset-pipeline work, outside this decision.
