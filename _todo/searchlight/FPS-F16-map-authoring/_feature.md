# FPS-F16 — The designer builds a Searchlight map in Blender and plays it

**Status:** Draft
**Labels:** feature, line:fps

## End state
Every Searchlight map is built in Blender with a Searchlight add-on ([GDD](https://github.com/WrackedFella/moho/blob/dev/wiki/searchlight/game-design-document.md) §5 Authoring). Each map component the game reads is placed from a Searchlight panel as a typed object with fields, checked inside Blender against the game's own rules, and played with one click. The add-on and the game read one component definition, so a feature that adds a component adds it to both. Users: the team; modders are GDD §9 Q34.

## Summary
The designer installs the add-on into one Blender LTS, places today's components (player spawn, enemy spawns, waypoints, cover with its direction, dummies) from a Searchlight panel instead of hand-naming empties, exports a glTF the game loads, presses Play to open the game on the map, and presses Check to see the problems the game would find. Today a map is a generator script or hand-named empties, and a mistake shows up only as a launch error.

**Moves toward the end state by:** making the add-on the place map components live before FPS-M3's first large map is built, which also fixes the kit's metrics (GDD §9 Q31). Loot points, building slots, safe spots, boundaries and anomalies join it with the features that use them.

## Exit criteria
- In Blender, the designer blocks out a new map and places every current component from the Searchlight panel without typing a marker name; the game loads the export and plays it: enemies start at the enemy spawns, patrol the waypoints and use the cover.
- Play opens the game on the map as it stands in Blender, unsaved changes included.
- Check lists the same problems, in the same words, that the game reports for that map, each naming its object; a sound map lists none.
- A component added to the shared definition appears in the panel and is known to the game's check, with no second list edited.
- The wiki states the supported Blender version and how to install the add-on.

## Scope
- In: the add-on (panel, typed components, export, Play, Check) for one Blender LTS; the shared component definition read by the add-on and the game; a way for the game to check a map without opening its window; install steps in the wiki.
- Out: an in-game editor; game behaviour inside Blender (enemies, physics, nav preview); kit pieces; components no feature reads yet; importing maps made before the add-on; packaging for modders.

## Direction-setting decisions
| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Editor | Blender plus a Searchlight add-on (Justin, 2026-10-29) | Native glTF and custom properties; same tool as placeholder art and rigs; handles large outdoor terrain, where brush editors are weak |
| Where the component list lives | One definition in the repo, read by both the add-on and the game | Two lists drift; a drifted list exports maps the game rejects |
| Where the map rules live | In the game; Check runs the game's own check | Rules written twice (Python and Rust) disagree, and the disagreement shows only at launch |
| Component shape | Point, oriented point or volume, with a kind chosen from game data and typed overrides on its defaults (GDD §5). Today's components use only points and oriented points | Volumes and data-driven kinds (anomalies) are built when a feature needs them, but the definition must not rule them out |
| Play's mode | The map's own mode (see the open question to Justin) | A forced sandbox needs a launch override in the game |
| Blender versions | One LTS | Each extra version is a test matrix row; the team picks the version |

## Deferred
| Idea | Why it waits | Revisit when |
|---|---|---|
| In-game marker overlay and live reload | Add to taste once a map is built with the add-on | After FPS-M3's first large map |
| Arena and proving ground moved from generator scripts to Blender files | Both load today; nothing breaks while they stay scripts | One of them next needs a layout change |
| Importing existing maps as typed components | Only script-made maps exist | With the arena move above |
| Kit pieces and snapping | Kit metrics are not fixed (GDD §9 Q31) | FPS-M3 first large map |
| Custom gizmos beyond Blender's empties and arrows | Empties and arrows show every current component | A volume component is hard to read with them |
| Collision-only meshes, invisible walls | Every mesh collides today and the arena needs nothing else | The first large map needs one |
| Modder packaging and a second Blender version | Users are the team (GDD §9 Q34) | §9 Q34 is answered |
| Anomalies, loot points, building slots, boundaries, safe spots | No feature reads them yet; each arrives with its feature | Their features (FPS-M5, FPS-M6, later) |

## Items
| Item | Behavior | Depends on |
|---|---|---|
| FPS-F16-01 | The designer places today's map components from a Searchlight panel and the game plays the exported map | ADR-0007 amendment |
| FPS-F16-02 | Play opens the game on the map open in Blender | FPS-F16-01 |
| FPS-F16-03 | Check lists the problems the game would find in the open map | FPS-F16-01 |

02 and 03 are independent of each other.

## Notes
- Milestone FPS-M2; independent of the other FPS-M2 features; done before FPS-M3's first large map starts.
- Level mode (arena or sandbox) becomes a component through FPS-F10-01 #627. Whichever of #627 and FPS-F16-01 lands second adds it to the add-on; the filing thread notes this on #627.
- TL: ADR-0007 bans copyleft, and an add-on using Blender's Python API is GPL. The add-on ships nothing into the game; amend the ADR to exempt tools outside the shipped build before FPS-F16-01 is built.
- TL: format and home of the shared component definition; how the game reads it.
- TL: how the add-on is tested and whether it joins the gate (Blender headless in CI, or tests on the parts that don't need Blender).
- TL: whether today's name-based markers stay as the export format or the game moves to typed properties; a map exported by the add-on and the current arena must both load during the move.
