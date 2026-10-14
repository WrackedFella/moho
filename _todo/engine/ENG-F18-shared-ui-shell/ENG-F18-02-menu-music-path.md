# Menu music plays from the assets folder

**Feature:** ENG-F18
**Status:** Draft
**Gate class:** unset
**Labels:** line:strategy

## Summary
The start menu looks for its music at `audio/music/menu.ogg`, outside `assets/` where the UI sounds live, so a music file placed beside them never plays. Found in #186's in-game checks.

## Deliverables
- The start menu plays music from `assets/audio/music/menu.ogg`.
- No music file means a silent menu and no error shown to the player.

## Acceptance criteria
```gherkin
Scenario: Menu music plays from beside the UI sounds
  Given a music file at assets/audio/music/menu.ogg
  When the start menu opens
  Then the menu music plays

Scenario: A missing music file leaves the menu silent
  Given no file at assets/audio/music/menu.ogg
  When the start menu opens
  Then no music plays
  And the menu works and shows no error
```

## Verification
1. Place any `.ogg` at `assets/audio/music/menu.ogg` and launch: music plays on the start menu.
2. Start a world, return to the start menu: the music keeps playing or restarts, without cutting out.
