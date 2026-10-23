# The player loads magazines from loose rounds, and mixed rounds fire in order

**Feature:** FPS-F6
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Loose rounds are a stack item keyed by caliber and round type, and each round type
carries its own damage. In the inventory screen the player loads a magazine from a
stack, one round at a time with a time per round, so mixing types is loading from
different stacks in turn (GDD §4.2). Depends on FPS-F6-02.

## Deliverables
- Two round types for the rifle's caliber, each with its own damage, in data.
- A hit deals the damage of the round that was fired.
- A load action fills a magazine from a stack of the same caliber until the magazine is
  full, the stack is empty or the player stops.
- Enemies' loadouts can carry loose rounds.

## Acceptance criteria
```gherkin
Scenario: Loading fills a magazine from a stack over time
  Given an empty 30-round magazine and a stack of 10 rounds in the backpack
  When the player loads the magazine and the load time for 10 rounds passes
  Then the magazine holds 10 rounds
  And the stack is gone

Scenario: Loading stops when the magazine is full
  Given a magazine with 1 free place and a stack of 10 rounds
  When the player loads the magazine
  Then the magazine is full
  And the stack holds 9 rounds

Scenario: Stopping keeps the rounds already loaded
  Given the player is loading a magazine from a stack of 10 rounds
  When the player closes the screen after 4 rounds' load time
  Then the magazine holds 4 more rounds and the stack 6 fewer

Scenario: Rounds of another caliber cannot be loaded
  Given a magazine and a stack of rounds of another caliber
  When the player tries to load the magazine from the stack
  Then nothing is loaded

Scenario: The last round loaded fires first
  Given a magazine loaded with 5 ball rounds and then 5 armor-piercing rounds
  When the player fires it
  Then the first 5 shots are armor-piercing and the next 5 are ball

Scenario Outline: A hit deals the damage of the round fired
  Given the chambered round is <type>
  When the player hits an enemy's torso
  Then the hit reports the <type> damage from data

  Examples:
    | type            |
    | ball            |
    | armor-piercing  |

Scenario: A mod adds a round type with no code change
  Given a mod that defines a new round type for the rifle's caliber
  When the player loads and fires it
  Then the hit reports the new type's damage
```

## Verification
- Load a magazine mid-wave and out of combat: the time per round makes loading in a
  fight a gamble. Record the time per round tried on this issue.

## Notes
- Round names are placeholders; damage per type is tuned in the playtest.
