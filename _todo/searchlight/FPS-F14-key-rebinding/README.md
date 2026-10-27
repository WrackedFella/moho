# FPS-F14 tech specs (drafts for review)

Tech Lead drafts for feature #718's cards. Each file holds the `## Tech spec` section to add
to its issue, and any change to the card it needs. On approval the specs go into the issue
bodies (which moves the cards to Ready on the board) and this directory is deleted.

Engine requests filed for this feature: #728 (Controls page rows) and #729 (double press).

## Build order

The cards share the Controls screen builder, the game's menu answers and the draft model, so
they run in sequence, not in parallel:

`#719 (01) -> #720 (02) -> #721 (03) -> #722 (04) -> #725 (07) -> #723 (05) -> #724 (06)`

- 02, 03 and 04 were independent in behavior; their footprints overlap in every file, so they
  are ordered. 02 first: it turns 01's refusal into the dialog that 03's scenario uses.
- 07 moves ahead of 05: it is smaller and needs only the checkbox part of #728. 05 and 06 stay
  the cut line, 06 cut first.

## Blocked

Every card waits on #728's core (capture, slots, clear, Unbound mark, update in place, long
list). #723 and #725 also wait on its details part; #724 also on #729. When the engine card
that answers #728 merges, each spec's "Engine seam" lines are checked against its real API
before the card is published.
