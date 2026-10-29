# Play opens the game on the map open in Blender

**Feature:** FPS-F16
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
A Play button in the Searchlight panel exports the open scene to a scratch file and launches the game on it, so the designer tries a change without saving, exporting or typing a path.

## Deliverables
- A Play button that launches the game on the scene as it stands, unsaved changes included.
- The game's location is set once in the add-on's preferences.
- A map the game cannot load shows the game's reason inside Blender.

## Acceptance criteria
```gherkin
Scenario: Play launches the open map with unsaved changes
  Given an exported map whose player spawn has since been moved and not saved
  When the designer presses Play
  Then the game opens on that map
  And the player starts at the moved spawn

Scenario: Play leaves the designer's files alone
  Given a saved scene and its last export
  When the designer presses Play
  Then the scene file and the last export are unchanged

Scenario: Play reports a map the game refuses
  Given a scene with no player spawn
  When the designer presses Play
  Then Blender shows the game's message that the player spawn is missing
  And no game window stays open

Scenario: Play without a game location asks for one
  Given the add-on's game location is not set
  When the designer presses Play
  Then Blender says where to set the game location
```

## Tech spec

## Verification
- Move a cover block and its cover point, press Play without saving, and see enemies use the new position.

## Notes
- The game plays the map in its own mode (arena or sandbox). See the feature's direction-setting decisions.
