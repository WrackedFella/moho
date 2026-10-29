# The designer places today's map components from a Searchlight panel and the game plays the exported map

**Feature:** FPS-F16
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
The walking skeleton: a Searchlight add-on for one Blender LTS whose panel places the components the game reads today as typed objects, and an export the game loads by path. The panel's component list is the shared definition the game also reads.

## Deliverables
- An installable Searchlight add-on with a panel listing every component in the shared definition.
- Each placed component shows as a typed object with its fields; no marker name is typed by hand.
- Export writes a glTF of the scene's meshes and components that the game loads by path.
- The shared component definition, read by the add-on and the game.
- A wiki page: supported Blender version, install, place, export.

## Acceptance criteria
```gherkin
Scenario Outline: The panel places each current component
  Given Blender with the Searchlight add-on installed
  When the designer adds a <component> from the Searchlight panel
  Then a <component> object appears at the 3D cursor
  And its panel shows the fields <fields>

  Examples:
    | component    | fields     |
    | player spawn | none       |
    | enemy spawn  | none       |
    | waypoint     | number     |
    | cover        | direction  |
    | dummy        | none       |

Scenario: Waypoints are numbered without typing
  Given a scene with waypoints numbered 1 and 2
  When the designer adds a waypoint
  Then the new waypoint is numbered 3

Scenario: The game plays an exported map
  Given a scene with a floor, walls, cover blocks, one player spawn, two enemy spawns, two waypoints, a cover point behind a block and a dummy
  When the designer exports the map and launches the game with its path
  Then the player starts at the player spawn, facing the way it faces in Blender
  And enemies start at the enemy spawns and patrol the waypoints
  And the dummy stands where it was placed
  And an enemy seeking cover hides behind the block on the side its direction names

Scenario: A component added to the definition reaches the panel
  Given a component added to the shared definition
  When Blender loads the add-on
  Then the panel offers that component with the definition's fields

Scenario: The current arena still loads
  Given the arena map that ships with the game
  When the game launches with no level path
  Then the arena plays as before
```

## Tech spec

## Verification
- Install the add-on into the supported Blender LTS from the wiki steps alone.
- Block out a small map, export it, launch the game on it, and play it: spawns, patrols, cover and dummy match the Blender layout.

## Notes
- Built only after the ADR-0007 amendment (GPL tools outside the shipped build) is accepted.
