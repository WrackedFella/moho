<!-- board: gate=unset; eligible=no; priority=P2 -->

# The player's rounds take time to arrive and fall over distance

**Feature:** FPS-F15 (#745)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
The walking skeleton of FPS-F15: the player's shot becomes a round that flies from the muzzle under gravity and drag, and hits whatever it reaches on the tick it gets there. A round definition carries its muzzle velocity and drag. Two tester controls: *Simple* brings back hitscan, and a gravity scale makes drop readable at arena range. Enemies keep hitscan until FPS-F15-03.

## Deliverables
- Player rounds fly, drop and slow; hits land on arrival; near misses count where the round passes.
- Round definitions carry muzzle velocity and drag, checked at load.
- Console commands for Simple (kept across launches, like god mode) and the gravity scale (1 at every launch).
- A replay test covering rounds in flight.

## Acceptance criteria
```gherkin
Scenario: A round is not a same-tick hit
  Given a dummy 60 m in front of the player
  When the player fires at its torso
  Then nothing is hit on the tick the shot is fired
  And the torso is hit on a later tick

Scenario: Further takes longer
  Given dummies 20 m and 60 m in front of the player, in line
  When the player fires one round at each
  Then the far dummy is hit more ticks after its shot than the near one

Scenario: A faster round arrives sooner
  Given two round definitions that differ only in muzzle velocity
  When each is fired at a dummy 60 m away
  Then the faster round hits first

Scenario: Drag slows a round
  Given two round definitions that differ only in drag
  When each is fired at a dummy 60 m away
  Then the round with more drag hits later

Scenario Outline: A round falls over distance
  Given the gravity scale is <scale>
  And a dummy 60 m away
  When the player aims level at the centre of its head and fires
  Then the round strikes <where>
  Examples:
    | scale | where                    |
    | 0     | the head at the aim point |
    | 1     | below the aim point       |
    | 30    | well below the head       |

Scenario: The gravity scale moves rounds only
  Given the gravity scale is 30
  When the player jumps
  Then the jump reaches the same height as at scale 1

Scenario: A round that hits nothing ends
  Given the player aims at open sky
  When the player fires
  Then the round leaves the level or slows below its minimum speed
  And no round from that shot remains in flight

Scenario: A near miss counts when the round passes
  Given a veteran enemy 60 m away, which takes cover after one near miss
  When the player's round passes within the near-miss radius of it
  Then the enemy is put under fire on the tick the round passes it, not the tick it was fired

Scenario: Restart clears rounds in flight
  Given player rounds are in flight
  When the tester restarts
  Then none of them hits anything after the restart

Scenario: Simple brings back hitscan
  Given Simple is on and the gravity scale is 30
  When the player fires at a dummy's head 60 m away
  Then the head is hit on the tick the shot is fired, at the aim point

Scenario: Simple is kept across launches
  Given the tester turned Simple on
  When the game is relaunched
  Then Simple is still on

Scenario Outline: A round definition is checked at load
  Given a round definition with <problem>
  When the content loads
  Then loading fails with a message naming the round and the field
  Examples:
    | problem                    |
    | no muzzle velocity         |
    | a muzzle velocity of 0     |
    | a negative drag            |

Scenario: A fight with rounds in flight replays exactly
  Given the same seed and the same commands
  When a fight in which the player fires rounds is run twice
  Then every hit lands on the same tick, part and point
```

## Tech spec

## Verification
1. Fire at a far wall at gravity scale 1, then 30: the second lands visibly lower. Turn Simple on: shots land at the crosshair again.
2. Play a wave in Flight and in Simple; hits still register and enemies still take cover under fire.

## Notes
- Rounds are not drawn until the engine tracer lands (request 2.18 on #636); hits and their feedback are the only cue.
