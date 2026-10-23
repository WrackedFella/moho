# The player carries and switches between a rifle, a pistol and an SMG

**Feature:** FPS-F7
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
The player wears weapons in equipment slots (primary 1, primary 2, sidearm) and switches
between them. A pistol and an SMG join the rifle, sharing a pistol caliber, so loose
rounds are keyed by caliber and magazines by weapon (#591). Body armor and helmet slots
are shown, empty. Depends on #596 (FPS-F6-02, inventory screen).

## Deliverables
- Equipment slots: primary 1, primary 2, sidearm, rig, backpack, body armor, helmet.
- A pistol and an SMG in one pistol caliber, each with its own magazine, in data.
- Weapon switch keys; switching takes the drawn weapon's draw time.
- The inventory screen lists the equipment slots; weapons equip from the backpack or a
  body into a free matching slot.
- Weapons carry a one-handed property in data (the pistol is one-handed); nothing reads
  it yet (GDD §9 Q24).
- Enemy loadouts can carry the new weapons.

## Acceptance criteria
```gherkin
Scenario: The player switches to the sidearm
  Given the player holds the rifle and wears a pistol in the sidearm slot
  When the player presses the sidearm key and the pistol's draw time passes
  Then the player holds the pistol
  And the next shot is fired by the pistol

Scenario: A weapon cannot fire while it is being drawn
  Given the player is switching to the pistol
  When the player pulls the trigger before the draw time passes
  Then no round is fired

Scenario: Pistol and SMG share rounds but not magazines
  Given a stack of pistol rounds, an empty pistol magazine and an empty SMG magazine
  When the player loads each magazine from the stack
  Then both magazines hold pistol rounds
  And the SMG refuses the pistol magazine on reload

Scenario: Reload looks only for the held weapon's magazines
  Given the player holds the SMG and the rig holds only rifle magazines
  When the player reloads
  Then no reload starts

Scenario Outline: A weapon equips only into a matching slot
  Given the backpack holds a <weapon> and the <slot> slot is free
  When the player equips the <weapon>
  Then the <slot> slot holds the <weapon>

  Examples:
    | weapon | slot      |
    | rifle  | primary 2 |
    | SMG    | primary 1 |
    | pistol | sidearm   |

Scenario: A long gun does not fit the sidearm slot
  Given the backpack holds an SMG and both primary slots are full
  When the player tries to equip the SMG
  Then the SMG stays in the backpack

Scenario: A looted weapon can be equipped from a body
  Given a dead enemy carried an SMG and the player has a free primary slot
  When the player equips the SMG from the body
  Then the primary slot holds the SMG

Scenario: A mod adds a weapon in an existing caliber
  Given a mod that defines a second rifle in the rifle caliber, with its own magazine, carried by enemies
  When the player kills such an enemy and equips its rifle
  Then the player can fire it, reload it from its magazines and load those from rifle rounds

Scenario: Armor slots are present and empty
  Given a run has started
  When the player opens the inventory
  Then the body armor and helmet slots are listed and empty
```

## Verification
- Fight a wave switching between all three weapons: switching reads clearly and each
  weapon feels distinct. Record the draw times tried on this issue.

## Notes
- Which slot a weapon fits is weapon data (long gun or sidearm), not per-weapon code.
