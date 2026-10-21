# Moho — Planning Index

Three project lines sharing one repo until the split
([ENG-F5](https://github.com/WrackedFella/moho/issues/232)). Card format, IDs and
lifecycle: [`_STANDARDS.md`](_STANDARDS.md). Each item's record is its GitHub issue, and its state lives on the
[project board](https://github.com/users/WrackedFella/projects/1); files under the line
folders are unapproved drafts or reference material. Architecture decisions:
[`adr/`](adr/README.md). Order of work: [`ROADMAP.md`](ROADMAP.md).

The Foundation gate ([ENG-F8](https://github.com/WrackedFella/moho/issues/61)) holds; feature work is open. Milestones
and their gates are in [`ROADMAP.md`](ROADMAP.md).

**Planning review checklist item: commercial licensing.** Decided 2026-10-02 in
[ADR-0007](adr/0007-third-party-licence-policy.md); the text below is the original prompt. Before the review closes,
decide how the shipped product will be licensed and sold, then audit the dependency
tree (`cargo deny list`, plus non-crate assets: fonts, audio, textures, shaders) for
terms that conflict. `deny.toml` already rejects GPL/LGPL and allows MPL-2.0 (file-level
copyleft), but it checks only crate licenses, not attribution duties or assets. Repeat
the audit at the v1 gate ([ENG-F5](https://github.com/WrackedFella/moho/issues/232)).

| Line | Folder | Focus |
|---|---|---|
| Engine | [`engine/`](engine/) | Shared crates; hygiene and debt land opportunistically |
| Strategy/base-builder | [`flagstone/`](flagstone/) | **Primary** |
| FPS | [`searchlight/`](searchlight/) | GDD only until it defines real scope |

## engine/

| Feature |
|---|
| [ENG-F1 Engine hygiene](engine/ENG-F1-engine-hygiene/_feature.md) |
| [ENG-F2 The dependency tree is current, clean and carries only chosen crates](https://github.com/WrackedFella/moho/issues/93) |
| [ENG-F3 Renderer pipeline cleanup](engine/ENG-F3-renderer-pipeline-cleanup/_feature.md) |
| [ENG-F4 Terrain rendering debt](engine/ENG-F4-terrain-rendering-debt/_feature.md) |
| [ENG-F5 Engine and games live in separate repos](https://github.com/WrackedFella/moho/issues/232) |
| [ENG-F6 Headless, deterministic logic](https://github.com/WrackedFella/moho/issues/233) |
| [ENG-F7 Entity storage is maintained and legion-free](https://github.com/WrackedFella/moho/issues/62) |
| [ENG-F8 Foundation gate](https://github.com/WrackedFella/moho/issues/61) |
| [ENG-F9 Domain tests catch behaviour changes](https://github.com/WrackedFella/moho/issues/234) |
| [ENG-F10 World geometry from any source](https://github.com/WrackedFella/moho/issues/80) |
| [ENG-F11 A game runs on the engine without copying the app loop](https://github.com/WrackedFella/moho/issues/81) |
| [ENG-F12 Input is game-defined actions bound as data](https://github.com/WrackedFella/moho/issues/82) |
| [ENG-F13 Every dependency has a reasoned verdict](https://github.com/WrackedFella/moho/issues/83) |
| [ENG-F14 A game loads a static level from a file](https://github.com/WrackedFella/moho/issues/226) |
| [ENG-F15 A ray finds the surface or body part it hits](https://github.com/WrackedFella/moho/issues/227) |
| [ENG-F16 Sounds play from positions](https://github.com/WrackedFella/moho/issues/228) |
| [ENG-F17 Agents path over any static level](https://github.com/WrackedFella/moho/issues/229) |
| [ENG-F18 Both games share the UI shell](https://github.com/WrackedFella/moho/issues/230) |
| [ENG-F19 Games load definitions and content from layered roots](https://github.com/WrackedFella/moho/issues/231) |
| [ENG-F20 A wgpu or egui upgrade touches one crate](https://github.com/WrackedFella/moho/issues/104) |
| [ENG-F21 A game gets a character controller and camera from the engine](https://github.com/WrackedFella/moho/issues/123) |
| [ENG-F22 Every test can fail and earns its place](https://github.com/WrackedFella/moho/issues/167) |
| [ENG-F23 The game knows what is under the pointer](https://github.com/WrackedFella/moho/issues/327) |
| [ENG-F24 The game marks places in the world and keeps them in view](https://github.com/WrackedFella/moho/issues/328) |
| [ENG-F25 A game line adds save files without changing the engine](https://github.com/WrackedFella/moho/issues/346) |
| [ENG-F28 Mouse look feels like a Source game](https://github.com/WrackedFella/moho/issues/373) |
| [ENG-F29 Light reads true in caves, at night and under open sky](https://github.com/WrackedFella/moho/issues/479) |
| [ENG-F30 Light bounces off what's around it](https://github.com/WrackedFella/moho/issues/480) |
| [ENG-F31 Mirrors and water reflect the world](https://github.com/WrackedFella/moho/issues/481) |
| [ENG-F32 Movement keeps its momentum and its footing](https://github.com/WrackedFella/moho/issues/491) |
| [ENG-F33 Movement rules are game data](https://github.com/WrackedFella/moho/issues/492) |
| [ENG-F34 Characters and bodies move each other](https://github.com/WrackedFella/moho/issues/493) |

## flagstone/

| Feature |
|---|
| [SG-F1 Possessing a pawn feels right](https://github.com/WrackedFella/moho/issues/311) |
| [SG-F2 A saved world comes back as I left it](https://github.com/WrackedFella/moho/issues/308) |
| [SG-F3 I place a building and pawns construct it](https://github.com/WrackedFella/moho/issues/312) |
| [SG-F4 Pawns dig what I mark](https://github.com/WrackedFella/moho/issues/310) |
| [SG-F5 I run the colony from above and step into a pawn](https://github.com/WrackedFella/moho/issues/309) |
| [SG-F6 New worlds look intentional and ore is findable](flagstone/SG-F6-world-generation/_feature.md) |
| [SG-F7 Light follows what I dig and build](https://github.com/WrackedFella/moho/issues/470) |

## searchlight/

| Feature |
|---|
| [FPS-F1 Game design document](searchlight/FPS-F1-game-design-document/_feature.md) |

## Shipped before this structure

Engine/game crate split: the game crate (now `flagstone_game`) created, renderer decoupled behind
`moho_render_api`, `AudioEvent` unified, wgpu 27→29.0.4. Lasting decisions are
in ADRs 0001–0003; the rest is in git history.
