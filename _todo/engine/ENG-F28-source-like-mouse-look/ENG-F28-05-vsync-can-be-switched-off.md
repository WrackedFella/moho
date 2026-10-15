# Vsync can be switched off

**Feature:** ENG-F28
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine, M2

## Summary
Video settings get a vsync choice. On is today's behaviour. Off presents frames with the
lowest latency the GPU offers, trading tearing for responsiveness.

## Deliverables
- A Video-tab vsync toggle, On by default, saved in prefs.
- Off uses the GPU's lowest-latency presentation that does not wait for the display. If the GPU has none, it keeps vsync and logs why.
- A change applies on Save without a restart.

## Acceptance criteria
```gherkin
Scenario: On by default
  Given no prefs file
  When the game starts
  Then frames wait for the display refresh

Scenario: Switching vsync off takes effect without a restart
  Given vsync is on
  When the player turns vsync off and saves
  Then later frames are presented without waiting for the display refresh

Scenario: A GPU without a low-latency mode keeps vsync
  Given the GPU offers only display-synchronised presentation
  When vsync is turned off
  Then frames still wait for the display refresh
  And the log says vsync stays on and why

Scenario: The setting survives a restart
  Given the player turns vsync off and saves
  When the game restarts
  Then vsync is off
```

## Tech spec

## Verification
- With vsync off, the debug HUD's frame rate runs above the display refresh, and mouse look feels more immediate than with vsync on.
