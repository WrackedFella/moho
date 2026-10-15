# ENG-F28 — Mouse look feels like a Source game

**Status:** Draft
**Labels:** feature, line:engine, M2

## End state
Both games turn the first-person view the way Source and Unreal shooters do: 1:1 with
the hand, every rendered frame, with no lag added by default. The settings a Source
player expects are in the menu. The simulation still receives look as a tick command
(ADR-0011), so multiplayer stays open.

## Summary
Mouse look in the FPS prototype feels stiff. The view turns only when a 60 Hz tick
runs, a deadzone throws away slow movement, and the default smoothing lags and drops
part of every motion. This feature makes look raw and per-frame, and adds the settings
that go with it: Source-scale sensitivity, invert Y, opt-in smoothing, opt-in
acceleration and a vsync choice.
**Moves toward the end state by:** giving the FPS line (and the strategy first-person
camera) a shooter-grade feel before FPS gameplay is tuned on top of it.

## Exit criteria
- The view turns on every rendered frame. At a display rate above 60 Hz, a steady
  mouse sweep shows no stepping at tick rate (manual playtest at 144 Hz).
- When a tick lands, the simulation's look equals what the camera already showed. The
  view never snaps back.
- Mouse movement that is slow or small always turns the view. The same counts always
  give the same turn, whatever the speed, as long as acceleration is off.
- Sensitivity uses Source's scale (0.022° per count × sensitivity), so a player can
  carry over their CS or HL2 value.
- In the Settings menu: sensitivity, invert Y, mouse smoothing (off by default),
  mouse acceleration (off by default) and vsync. Each one survives a restart and takes
  effect without one.
- Gamepad stick look also turns per frame, and keeps its stick deadzone.
- Strategy first-person and isometric controls behave as before, apart from the
  smoother turning (manual check).

## Scope
- In: first-person look in both games (FPS-prototype walking and the strategy
  free-cam), mouse and right stick; the five settings above, on today's Settings
  screens.
- Out: aim assist; per-weapon or zoom sensitivity; frame-rate cap; cm/360 readout;
  rebinding (ENG-F26, ENG-F18).

## Direction-setting decisions
| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Where does look live between ticks? | Presentation shows look the simulation hasn't consumed yet. The next tick's command carries exactly that amount, so the simulation ends where the camera already is. | Keeps ADR-0011 (commands drive the tick). Moving look out of the simulation would break hit tests and later networking. |
| Delta or absolute angles in the command? | Open for the Tech Lead. Either is fine if the view never snaps back. | Source sends absolute angles; deltas are what exists today. |
| Default feel | Raw: no mouse deadzone, no smoothing, no acceleration. | Source and competitive Unreal games ship this way. Smoothing hides tick steps but adds lag. |
| Sensitivity scale | Source's scale, default 3.0 (HL2's default, ~17 cm/360° at 800 DPI). The existing prefs value is read on the new scale, with no migration. | Familiar numbers for players. Today's 1.0 equals Source ≈5.2, which is fast. Only dev prefs files exist, so a migration buys nothing. |
| Smoothing when on | Averages recent samples and never loses movement: the total turn equals the hand's total motion. | Today's filter drops about 20% of each motion when it stops. |
| The current "Input filtering" toggle | Replaced by "Mouse smoothing". The old prefs key is ignored with a warning. | The toggle bundled the deadzone (removed) and smoothing (kept) under one name. |
| Acceleration model | Source `m_customaccel`-style: turn grows with mouse speed, one strength value, off by default. | One knob covers what players ask for. Source's extra exponent and cap knobs wait. |
| Vsync | Choice of On or Off; On is the default. Off uses the lowest-latency mode the GPU offers. | Today it is always on (Fifo), which adds 1 to 2 frames of latency. Defaulting to Off would tear on first launch. |
| Placement | Engine: input, prefs, camera and renderer. Both games reuse it as-is. | Passes the placement test. The settings move with ENG-F18 when the shell moves. |

## Deferred
| Idea | Why it waits | Revisit when |
|---|---|---|
| Frame-rate cap (`fps_max`) | Vsync Off runs uncapped. No one needs a cap yet. | Someone reports heat or noise, or wants a cap for consistency. |
| cm/360 readout with a DPI field | It's polish. The Source scale already lets players carry their values over. | FPS settings polish. |
| Separate yaw/pitch scale, zoom/ADS sensitivity | There's no ADS consumer yet. | The FPS line adds aiming down sights. |
| Acceleration exponent and cap | One strength value covers the request. | Players ask for finer tuning. |
| Aim assist for pads | It's game design, owned by the FPS line. | The FPS line files an engine request. |

## Items
| Item |
|---|
| ENG-F28-01 Slow and small mouse movements always turn the view (no mouse deadzone; smoothing opt-in and loss-free; replaces Input filtering) |
| ENG-F28-02 The view turns every rendered frame (mouse and stick; walking and free-cam) |
| ENG-F28-03 Sensitivity uses Source's scale, and Y can be inverted |
| ENG-F28-04 Mouse acceleration can be switched on |
| ENG-F28-05 Vsync can be switched off |

Order: 01 → 02 (02 must show exactly what the tick will consume, including smoothing).
03, 04 and 05 are independent of each other and of 01 and 02. 04 reads best after 03.

## Notes
- 01, 03, 04 and 05 add prefs and Settings controls. They queue behind the wave-1
  prefs and settings cards (#331, #333, #354), which touch the same files.
