
Verdict: reject
Route: fix
Sign-off: human
Head: 83969a65598744c2cb07d7dbcc8cee661dac2c15   Tier: light   Round: 1

Blockers
[X1] B2, searchlight/src/game.rs:1907 (lookup at game.rs:956-961)
  Failure scenario: the criterion "Given the player is within reach of the next-wave button and looking at it / Then a prompt names the use key and \"Next wave\"" is only tested in separate pieces. No test checks the prompt text the game actually shows. `the_prompt_shows_while_the_button_is_in_reach` only checks that some text is on screen (`ui_frame(..).is_some()`). `hud::tests::the_use_prompt_names_the_key_and_next_wave` passes the literal "F" in as the key. So nothing tests the binding lookup in `show_use_prompt`, which per the tech spec should use "the first `Use` binding's `label()`, \"unbound\" when there is none".
  Evidence: the refuter changed the lookup to read `SearchlightAction::Reload` and replaced the "unbound" fallback with an empty string. `cargo nextest run -p searchlight` still passed: 581 passed, 0 skipped.
  Expected fix: add a game-level test that reads the prompt text the frame puts on screen. It should cover the default bindings ("[F] Next wave") and an unbound `use` ("[unbound] Next wave"). One way is to have `show_use_prompt` build its text through a small function the test can call.

Questions
  none

Follow-ups
  none

Sign-off triggers
  - 1292 changed lines, over 400 excluding lockfiles

Dropped
  none

Not reviewed
  - Light tier: the defect-finder did not run, and no one ran the gate (`just check`) or `just mutants` on the whole change. Only the refuter's targeted `cargo nextest run -p searchlight` ran.
  - The binary contents of searchlight/assets/arena.glb were not inspected. The existing shipped-arena tests cover the file.
  - The ADR texts in _todo/adr/ were checked only through the summaries in CLAUDE.md.
  - No CI results exist for this range (dry run).

Counts
  candidates 1, dropped 0, blockers 1, follow-ups 0, agent calls 2, design-critic model default

