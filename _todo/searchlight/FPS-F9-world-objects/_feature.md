# FPS-F9 — The player uses what they look at: buttons, dropped items and bodies answer one use key

**Status:** Draft
**Labels:** feature, line:fps

## End state
The zone is full of things the player handles: magazines and kit lying where they fell,
bodies to search, caches, switches and doors, and later the workbench (FPS-M3), the bed
(FPS-M5) and the trader and job board (FPS-M6). Every one is used the same way: look at
it, get within reach, read the prompt, press use. The host resolves each use for each of
up to four co-op players ([GDD](../../../wiki/searchlight/game-design-document.md) §3.3,
§4.1, §5).

## Summary
Ships the shared use mechanic with its first kinds of world object: a control placed in
the map that fires an arena action (the next-wave button), loose items that fall, settle
and are picked up (dropped items and fast-reloaded magazines), and bodies that stay where
they died and are searched in place. Today each consumer card (body search, equipment
table, dropping items) would grow its own look, reach and prompt; this feature builds
that once in its first slice and the consumers reuse it.
**Moves toward the end state by:** making every later station (workbench, bed, trader,
cache) one new action on an existing mechanic, not a new mechanic.

## Exit criteria
- One use key, one look-and-reach rule and one prompt serve the next-wave button, a
  loose magazine and a dead enemy; no consumer card adds its own.
- A level author adds a working control to a level by placing a marker that names an
  action; no code changes.
- A dropped magazine leaves the player, falls, settles on the floor and is picked up by
  looking at it.
- A use sent from any player slot is resolved by the simulation from that player's own
  eye and reach, and host actions (next wave) are honored from slot 0 only.

```gherkin
Scenario: One prompt for every kind of world object
  Given the player looks at a <object> within reach
  Then the prompt names the use key and "<action>"

  Examples:
    | object             | action    |
    | next-wave button   | Next wave |
    | loose magazine     | Pick up   |
    | dead enemy         | Search    |
```

## Scope
- In: the use key, the look-and-reach test, the prompt, uses as per-player commands
  resolved on the tick; controls placed in a level by marker, each naming one action from
  a closed list the line offers; loose physics items (engine #600); bodies as world
  objects that keep their place and contents; ragdoll as a deferred slice.
- In, built by the consumer that first needs it: a use that takes time and cancels on
  looking away or leaving reach (body search, FPS-F6-02 #596).
- Out: doors and other moving level geometry; carrying, dragging, throwing, kicking or
  shooting objects to move them; locks and keys; chained events or scripting; more than
  one action per object; hand animation (FPS-F8).

## Direction-setting decisions
| Question | Decision | Why / cost of the alternative |
|---|---|---|
| One mechanic, or one per system? | One, for objects placed in the level and objects that appear at runtime (dropped items, bodies) | Per-system prompts and reach rules drift apart and each needs its own co-op path |
| What can a placed control do? | Name one action from a closed list the line offers (next wave, reset dummies, spawn an enemy, give items); the level picks the action and its values | General event wiring or scripting costs a tool with no consumer; the closed list grows one action at a time |
| Who decides what was used? | The client sends "use"; the host runs the look-and-reach test from that player's eye on the tick. The prompt is local, from the same test | A client naming its target is a hit claim (multiplayer direction: host-resolved). Matches the command-driven sim (ADR-0011) |
| Two players use one object on the same tick | The lower slot wins; the other use does nothing | Deterministic and replayable; a queue or a share rule can come with co-op design |
| Does the sim wait for animation? | No. Uses resolve on the tick, instantly or after a use time the object sets; FPS-F8 hand animation plays over it. Reload is the one place a clip is fitted to a sim time (#543) | Animation-driven timing ties outcomes to presentation and breaks headless replay |
| Ragdoll | Changes where a body lies and how it looks, never what it holds or how it is searched | Lets ragdoll slip without touching looting |

## Deferred
| Idea | Why it waits | Revisit when |
|---|---|---|
| Ragdoll bodies (Justin prefers) | Needs a new engine request (jointed bodies that fall limp); looting works without it | The engine schedules the request; FPS-F9-05 |
| Doors, hatches, moving level parts | Moving colliders and navigation; no consumer before zones | FPS-M6 zone maps, as an engine request |
| Caches and crates placed in the level | Bodies are the only container the arena needs | FPS-M6 loot points (GDD §5) or an arena cache test |
| Dragging bodies, carrying or throwing objects | Second interaction per object; no loop needs it | After ragdoll, if playtest asks |
| Kicking or shooting objects, impact sounds | #600 cut lines (ENG-F34 territory) | ENG-F34 |
| More than one action per object (search or drag) | Needs a choice UI | A second action exists |
| Highlight outline on the looked-at object | Polish; the prompt shows the target | Playtest says targets are hard to read |
| Locks, keys, use from a hotkey | No consumer | Zones |

## Items
| Item | Behavior | Depends on |
|---|---|---|
| FPS-F9-01 | The tester presses an in-world button to call the next wave (the walking skeleton: use key, look and reach, prompt, placed control, host action) | none |
| FPS-F9-02 | A level author wires a control to any action on the closed list by its marker (reset dummies, spawn an enemy of a grade, give items) | FPS-F9-01 |
| FPS-F9-03 | The player drops an item, it lies where it fell, and looking at it picks it up (static, under the drop point) | FPS-F9-01; FPS-F6-02 #596 (inventory screen) |
| FPS-F9-04 | Dropped items fall, tumble and settle before they are picked up | engine #600; FPS-F9-03 |
| FPS-F9-05 (deferred) | A killed enemy falls limp where the shot pushed it, and is searched where it lies | ragdoll engine request; FPS-F6-02 #596 |

Consumers that stay in their own features and gain a dependency:

| Card | Keeps | New dependency |
|---|---|---|
| FPS-F6-02 #596 search a body | the timed search, the inventory screen | FPS-F9-01 (use key, reach, prompt) |
| FPS-F6-04 #598 equipment table | what the table hands out | FPS-F9-02 (each spawner is a placed control) |
| FPS-F7-03 #606 weight | weight, load and speed; drop and pick-up move to FPS-F9-03 | FPS-F9-03 |
| FPS-F7-06 fast reload (draft) | both reload variants | FPS-F9-03 (the dropped magazine), not #606 |

## Notes
- Milestone FPS-M2. FPS-F9-01 has no dependency and goes before #596 and #598.
- Animation (FPS-F8, hands and reload): adds a card where the hand reaches for and takes an
  item the player picks up (FPS-F9-03), and the fast-reload magazine leaves the hand as a
  loose object (FPS-F9-04). Both play over outcomes the sim already resolved.
- Arena controls are mode-layer scaffolding (GDD §3.2); the mechanic itself is not.
- TL: is the look-and-reach test a line concept, or does the flagstone pawn's tool raycast
  make it engine-generic? If generic, it is an engine request stating the need.
- TL: the prompt needs on-screen text (#407); until it lands, the title-bar HUD carries it.
- TL: the use key joins the keybinding set FPS-F3-03 #460 shows and ENG-F26 #359 rebinds.
