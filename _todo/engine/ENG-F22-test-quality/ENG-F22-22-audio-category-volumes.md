# Every audio category's volume and mute reach the effective volume

**Feature:** ENG-F22
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary

Audio settings tests cover music only, so mutants in the sound-effects, UI and voice
volume setters survive (found in #177, ENG-F22-10). Pin each category through the
effective volume a sound plays at.

## Deliverables

- One table-driven test covering every audio category, replacing the music-only cases
  where they duplicate it.
- `cargo mutants --file` on the audio settings source shows no surviving setter mutant.

## Acceptance criteria

```gherkin
Scenario Outline: A category's volume scales by master volume
  Given master volume 0.5
  And the <category> volume is set to 0.5
  When the effective volume of a <category> sound is read
  Then it is 0.25
  And every other category keeps its default volume times 0.5

  Examples:
    | category      |
    | sound effects |
    | music         |
    | UI            |
    | voice         |

Scenario Outline: A category volume outside 0..1 is clamped
  Given the <category> volume is set to 1.5
  When the effective volume of a <category> sound is read at master volume 1.0
  Then it is 1.0

  Examples:
    | category      |
    | sound effects |
    | music         |
    | UI            |
    | voice         |

Scenario: Mute silences every category
  Given every category volume is 1.0 and audio is muted
  When the effective volume of each category is read
  Then each is 0.0
```

## Notes

- The audio system itself (device, playback, event routing) stays untested here: it
  needs an output-device seam, and ENG-F16 (sounds play from positions) reworks it.
