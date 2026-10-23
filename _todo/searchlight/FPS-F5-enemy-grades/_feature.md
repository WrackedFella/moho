# FPS-F5 — Enemies fight at five skill grades, and later waves field better fighters

**Status:** Draft
**Labels:** feature, line:fps

## End state

Enemies read as people of differing skill who act as units under a strategic Director
([GDD §4.11](../../../wiki/searchlight/game-design-document.md#411-enemy-ai-grades-groups-and-factions)).
Skill changes behavior, never toughness: a rookie and a professional die to the same hits.

## Summary

Every arena enemy has one of five grades (rookie, novice, experienced, veteran,
professional). The grade sets how fast it opens fire, how well it aims, how steadily it
keeps firing and how soon it gets into cover. Waves escalate by shifting the grade mix up
the ladder, not by making enemies tougher. Today's single enemy profile becomes
*experienced*, so nothing that works now changes until the mix does.

**Moves toward the end state by:** giving slice 2 (group alerts, leader lift, fire and
maneuver; needs #229 cover and exposure queries) a grade ladder to lift and coordinate,
and Phase 2 a grade to hang gear and loot off.

## Exit criteria

- In a playtest with grades hidden, Justin tells a rookie from a professional, and a
  rookie from an experienced enemy, in the first fight with each.
- Enemies of every grade die to the same hits and deal the same damage per hit.
- Later waves field a visibly larger share of experienced-and-above enemies than wave 1.
- Headline behavior, assertable:

```gherkin
Scenario: A professional opens fire before a rookie
  Given a rookie and a professional each spot the standing player at the same range
  When both engage
  Then the professional fires its first shot before the rookie does

Scenario: A professional is in cover before a rookie
  Given a rookie and a professional stand in the open near cover
  When the player fires at each
  Then the professional reaches cover before the rookie does

Scenario: Later waves sit higher on the ladder
  Given wave 1 and wave 5 of the same run
  When each wave's enemies spawn
  Then wave 5 has a larger share of experienced, veteran and professional enemies
```

## Scope

- In: five grades as one ordered ladder of data; three execution dials per grade
  (reaction time, aim error, fire cadence); when an enemy takes cover, per grade; the
  wave's grade mix shifting up over waves; a console command that forces the grade of
  new spawns, for playtest and tuning; today's enemy profile as *experienced*.
- Out: a grade shown to the player (HUD, label, colour, gear); anything about groups,
  leaders, relocation, pushing or holding; difficulty settings.

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| What sends an enemy to cover? | Being shot at (a near miss) as well as being hit; low grades react late or only to hits | Today only a hit sends an enemy to cover. Hits-only is cheaper but rookie and pro both die or hide on the first hit, so the tell is weak. Near misses are new perception, and slice 2 suppression needs them anyway |
| Grade fixed for an enemy's life? | Yes (GDD §4.11) | Rank-up is a Phase 7 Director nice-to-have |
| Does grade change damage dealt or health? | No, neither | Pillar: no bullet sponges (GDD §6); difficulty shifts the mix only |
| "Pauses before or between bursts" | One dial: fire cadence. Lower grades fire fewer shots over the same time | Enemies fire single shots today; bursts are new weapon behavior with no other caller yet |
| How a wave's mix is set | Data, tunable per wave; wave 1 is mostly rookie and novice | Hard-coded mixes block tuning; a formula is the TL's call |
| How tuning is verified without a label | A console command forces the grade of new spawns | The exit test hides grades, so tuning needs another lever; the console already exists |
| Five grades distinct now? | Exit checks the two ends and the middle; neighbours tuned later | Five steps need many playtests to separate; GDD §4.11 accepts neighbours showing over several fights |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| Strength number for the offline sim | Nothing reads it until the Director | Phase 7 (FPS-M7) |
| Relocating under fire, push or hold, flanking | Needs groups and cover/exposure queries (#229) | Slice 2, before FPS-M3 |
| Group alerts, leader lift, fire and maneuver | Slice 2 | Before FPS-M3 |
| Gear and loot by grade | Needs the item model | FPS-M2 (Phase 2) |
| Bursts and suppressing fire | No burst weapon behavior yet; suppression is slice 2 | Slice 2 |
| Wounds modifying execution | Needs enemy wounds (§4.7) | FPS-M4, to taste |
| Legend grade | Reserved for named NPCs | Named NPCs exist |
| Callouts that announce a grade's moves | Callouts belong to group tactics; needs positional audio (#228) | Slice 2 |
| Difficulty setting that shifts the mix | GDD §9 Q8 still open | Q8 decided |
| Grade in the HUD | Breaks the "read it from behavior" test | Only if playtest says grades are unreadable |

## Items

| Item | Delivers | Depends on |
|---|---|---|
| FPS-F5-01 Enemies open fire, aim and keep firing by grade | Grade ladder; reaction, aim, cadence per grade; toughness and damage unchanged; console forces the grade; default spawns are *experienced* | none |
| FPS-F5-02 Better-graded enemies take cover sooner | Cover timing per grade; near misses count as being under fire | -01 |
| FPS-F5-03 Later waves field better-graded enemies | Wave grade mix shifting up the ladder | -01 |

-02 and -03 are independent of each other.

## Notes

- TL: per-enemy aim error multiplies the spread offset after the draw (#456 needs no
  change); reaction and cadence draws get their own named random streams so they don't
  shift the spread stream.
- TL: one grade definition in data, readable later by the offline sim and the item model
  (GDD §10 item 7).
- TL: if cover is triggered by near misses, how a shot that passes close to an enemy
  reaches it as a percept.
