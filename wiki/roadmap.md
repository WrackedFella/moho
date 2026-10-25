# Moho roadmap

Coordinates the three lines. Each line's own order of work lives in its roadmap and never refers to another line's
decisions; effects that cross lines are recorded here as dependencies. Item status is not tracked in any roadmap; it
lives on the [project board](https://github.com/users/WrackedFella/projects/1) and in the issues.

| Line | Roadmap | Milestones | Holds |
|---|---|---|---|
| Engine | [engine/roadmap.md](engine/roadmap.md) | ENG-M0 to ENG-M5+ | Engine milestones and gates, waves, out-of-scope, backlog, engine rules |
| Flagstone (strategy) | [flagstone/roadmap.md](flagstone/roadmap.md) | STRAT-M1 to STRAT-M4 | Strategy features in order, gates, out-of-scope |
| Searchlight (FPS) | [searchlight/roadmap.md](searchlight/roadmap.md) | FPS-M1 to FPS-M7 (FPS-M1.1 and FPS-M1.2 are bridges) | FPS milestones, entry gates, exit goals, engine requests |

Update this page when a gate that spans lines changes, or when one line starts or stops depending on another. Update
a line's roadmap when a gate closes, an item moves, or a decision is made inside that line.

Milestone names are `<LINE>-M<n>`: ENG, STRAT or FPS. ISO 2145 numbering applies (FPS-M1.1 is a bridge after FPS-M1).
Issue labels such as `M2` mean ENG-M2.

## Milestone gates that span lines

| Gate | Holds when | Line items involved |
|---|---|---|
| ENG-M2 (engine MVP) | The engine carries the current scope of both games and the save envelope is in use | FPS Phase 1 and 2 playable on engine crates (FPS-M1, FPS-M2 entries); SG-F1, SG-F2, SG-F4 and SG-F5 done (STRAT-M1 to STRAT-M3; the rest of SG-F1 sits in STRAT-M4 but is still required). Declared met by Justin |
| ENG-M3 (v1.0 and split) | The ENG-F5 gate holds: licences and the third-party notices file; the seam list in ADR-0012 is unchanged by the last two FPS features; the `v1.0` tag; ADR-0011 two-simulation proof | The last two FPS features before the split are FPS-M4.1 and FPS-M5 ([prototype gate](searchlight/roadmap.md#13-prototype-gate)); the split follows FPS-M5 completion or the ENG-F5 gate, whichever is later. Every engine seam the prototype uses, and level unload for FPS-M6, lands before FPS-M4.1 starts (engine roadmap rows 2.20 to 2.25 and 3.4b to 3.4g) |

STRAT-M1 to STRAT-M4 run inside ENG-M2. STRAT-M1 and STRAT-M2 may overlap.

## Dependencies: lines need engine items

Engine item numbers are those of the [engine roadmap](engine/roadmap.md).

| Line item | Needs | Engine item |
|---|---|---|
| SG-F1 possession feel | Momentum kept when leaving the ground | 2.1 ENG-F21-03 |
| SG-F5 click select, SG-F4 mark, SG-F3 place | Pointer-to-world ray | 2.6 ENG-F23 |
| SG-F5, SG-F4, SG-F3 | World markers; SG-F3 also the placement preview | 2.7 ENG-F24 |
| SG-F4 | Domain tests that catch behaviour changes | 2.8 ENG-F9 (done) |
| SG-F4 marks persist | Game-owned save kinds | 2.5.1 ENG-F25 (before the marks-persist card) |
| SG-F4 follow-ups | Pause and 1x/2x/3x speed; hide the world above a height | 2.9 ENG-F11-04; 2.10 ENG-F24-03 |
| SG-F7-04 first emissive block | World markers (ENG-F24-05, #469) | 2.7 |
| SG-F7-05 smooth sky light | Light composition | 2.17 ENG-F29-01 (lands with SG-F7-05) |
| SG-F2-03 autosave interval | Player-set autosave | 2.11.5 ENG-F18-16 |
| Strategy console | Shared console | 2.11.2 ENG-F18-08 |
| FPS-M1 | Seeded RNG; positional audio (stopgaps in use) | 2.5 ENG-F6-01; 2.4 ENG-F16 |
| FPS-M1.1 menus | Menu screens and their stack; engine settings controls | 2.11.6 ENG-F18-11, -12 (spec: #444) |
| FPS-M1.2 slice 2 | Crouch and clearance; lean and stance transitions | ENG-F43 #642, crouch, prone and lean, answering #544 (2.25, ENG-M2 wave 5) |
| FPS-M1.2 slice 3 | Skinned animated characters | #543 through ENG-F39 slices 1 to 3 (2.22, ENG-M2 wave 5) |
| FPS-M2 | On-screen text; layered content; tap and hold keys; item lists | 2.11.3 ENG-F18-09 #407; 2.12 ENG-F19 #231; 2.14 ENG-F26 #359; 2.11.6 ENG-F18-18 |
| FPS-M2 | Moving meshes; loose objects; models, sockets and rig; flash, tracer and impact marks; marker size | 2.20 ENG-F38; 2.21 #600; 2.22 ENG-F39; 2.23 ENG-F40; 2.24 ENG-F14-03 |
| FPS enemy AI slice 2 | Positional audio; hidden routes and cover | 2.4 ENG-F16; 3.4c ENG-F42 |
| FPS-M3.1 | Beam and dot; first-person view | 3.4d ENG-F40; 3.4b ENG-F39 |
| FPS-M4 | Screen and sound follow game values; blended, sim-timed clips | 3.4e ENG-F41; 3.4b ENG-F39 |
| FPS-M5 | Game clock scaling; save and load | ENG-F11-04 and the game clock; 2.5.1 ENG-F25 |
| FPS-M6 | Level switching carrying player state | 3.4f level unload and switch (before the split) |
| FPS-M7 | Headless simulation; navigation for live actors | ENG-F6 #233 related; ENG-F17 |

## Dependencies: engine items serve lines

| Engine item | Serves | Order |
|---|---|---|
| 3.2 ENG-F30 light bounce | Strategy terrain first, then sun, emissive and FPS voxelisation | Skeleton on strategy terrain; FPS slice follows |
| 3.2.1 ENG-F35 sim knows how lit a place is | FPS stealth | Slice 4 (coarse bounce grid) after ENG-F30's FPS voxelisation slice |
| 2.17 ENG-F29 light composition | SG-F7-05, searchlight arena ambient | -01 lands with SG-F7-05 |
| 2.18 ENG-F31 mirrors, SSR, water | Searchlight arena mirror and pool | After ENG-F29-04 |
| 2.19 ENG-F32 movement | FPS feel, flagstone possession | `moho_physics` only; runs beside renderer and UI items |
| 2.16 ENG-F28 mouse look | FPS feel, strategy first-person camera | Wave 2 |

## Cross-line rules

- A line never depends on the other line. Shared needs go to the engine as engine requests (issue label
  `engine-request`).
- An engine request closes when the engine work that answers it merges; the line's card tracks adoption.
- Game AI stays in each line. The engine offers queries (navigation, rays, gameplay light).
- Lighting is one dynamic engine path for both lines.
- The FPS and strategy lines are not documented in each other's sections. Shared pages: this one,
  [architecture](architecture/overview.md), [reference](reference/), [process](process/agentic-workflow.md) and the
  [ADRs](../_todo/adr/README.md).
