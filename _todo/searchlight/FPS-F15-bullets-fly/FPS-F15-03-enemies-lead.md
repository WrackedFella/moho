<!-- board: gate=unset; eligible=no; priority=P2 -->

# Enemies fire flying rounds and lead a moving player by grade

**Feature:** FPS-F15 (#745)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Enemy shots become flying rounds like the player's (FPS-F15-01). To hit a moving player an enemy aims where the player will be when the round arrives, from the player's speed when it fires; the grade scales how wrong that lead is, as it already scales aim error.

## Deliverables
- Enemy rounds fly, drop and slow, and hit on arrival.
- Enemies lead a moving player; a lead-error dial per grade.
- Simple brings back enemy hitscan with no lead.

## Acceptance criteria
```gherkin
Scenario: An enemy round takes time to arrive
  Given an enemy 40 m from a standing player
  When the enemy fires
  Then the player is not hit on the tick the shot is fired
  And the player is hit on a later tick

Scenario: A perfect enemy leads a running player
  Given an enemy with no aim or lead error, 40 m away
  And the player runs across its view at a steady speed
  When the enemy fires
  Then the round hits the player

Scenario: Without lead the same shot misses
  Given the same enemy and run, with lead turned off for the test
  When the enemy fires
  Then the round passes behind the player

Scenario: Stopping beats the lead
  Given an enemy with no aim or lead error fires at the running player
  When the player stops before the round arrives
  Then the round passes in front of the player

Scenario: A higher grade leads better
  Given the same seed and a player running across at 40 m
  When a rookie and a professional each fire 50 rounds
  Then the professional hits the player more often

Scenario: A standing player sees no lead
  Given a standing player
  When an enemy fires
  Then it aims at the player, as it does today

Scenario: Simple brings back enemy hitscan
  Given Simple is on
  When an enemy fires at the player
  Then the hit or miss is decided on the tick the shot is fired, with no lead

Scenario: God mode still holds
  Given god mode is on
  When an enemy round hits the player
  Then the player takes no damage, as today

Scenario: A fight with enemy rounds replays exactly
  Given the same seed and the same commands
  When a fight in which enemies fire is run twice
  Then every hit on the player lands on the same tick, part and point
```

## Tech spec

## Verification
1. Strafe across a veteran's view at range; you are hit while running and the hits stop landing when you stop short. A rookie misses a running player far more often.

## Notes
- An enemy round that reaches another enemy does what an enemy shot does today (TL to confirm the current rule).
- Lead uses the player's velocity, not their acceleration; a player who turns or stops can dodge.
