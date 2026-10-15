# Mouse acceleration can be switched on

**Feature:** ENG-F28
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine, M2

## Summary
Players who want acceleration can turn it on in Settings, in Source's `m_customaccel`
style: the faster the mouse moves, the more each count turns. One strength value sets
how much. It is off by default.

## Deliverables
- A "Mouse acceleration" toggle (off by default) and a strength value, both saved in prefs and applied on Save.

## Acceptance criteria
```gherkin
Scenario: Off by default, so speed does not matter
  Given no prefs file
  When the same number of counts is moved slowly and then quickly
  Then both movements turn the view by the same amount

Scenario: On, fast movement turns further
  Given mouse acceleration is on with a strength above zero
  When the same number of counts is moved slowly and then quickly
  Then the quick movement turns the view further

Scenario: On with zero strength behaves as off
  Given mouse acceleration is on with strength 0
  When the same number of counts is moved slowly and then quickly
  Then both movements turn the view by the same amount

Scenario: The settings survive a restart
  Given the player turns acceleration on, sets a strength and saves
  When the game restarts
  Then acceleration is on with that strength
```

## Tech spec

## Verification
- With acceleration on, fast flicks travel further than a slow sweep of the same mouse distance.
