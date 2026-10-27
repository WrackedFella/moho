<!-- board: gate=unset; eligible=no; priority=P2 -->

# Rounds in flight are drawn as tracers

**Feature:** FPS-F15 (#745)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps
**Note:** waits on ENG-F38 #634 and ENG-F40 #636 slice 1 (the followed effect).

## Summary
Every round in flight is drawn as a tracer that follows it, so travel time, drop and a bounce can be seen. The game uses the engine's followed effect from ENG-F40 slice 1: it places the tracer each frame and ends it when the round's flight ends.

## Deliverables
- A tracer for each player round in flight, and for each enemy round once FPS-F15-03 is in.
- The tracer turns with a bounce (FPS-F15-04) and ends when the round stops.

## Acceptance criteria
```gherkin
Scenario: A round in flight has a tracer
  Given the player fires
  When the round is in flight
  Then one tracer is shown at the round's position

Scenario: The tracer ends with the round
  Given a round in flight with its tracer
  When the round hits, slows below its minimum speed or leaves the level
  Then its tracer is gone

Scenario: A bounce turns the tracer
  Given a round bounces off a wall
  Then its tracer carries on along the new heading

Scenario: Restart clears every tracer
  Given rounds are in flight
  When the tester restarts
  Then no tracer remains

Scenario: Simple shows no tracers in flight
  Given Simple is on
  When the player fires
  Then no tracer follows a round
```

## Tech spec

## Verification
1. Fire across the arena at gravity scale 1 and 30: the tracer arcs visibly at 30 and moves smoothly at any frame rate.
2. Bounce a round off a wall: the tracer turns at the wall.
3. Strafe in front of a veteran: its tracers lead you.

## Notes
- Tracer look (length, colour, Video settings) is the engine's; the game supplies position and heading only.
