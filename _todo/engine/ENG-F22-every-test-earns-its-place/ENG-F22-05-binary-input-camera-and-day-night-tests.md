# Binary input, camera and day-night tests fail for wrong implementations

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#172](https://github.com/WrackedFella/moho/issues/172)
**Status:** Backlog
**Gate class:** glue
**Labels:** line:engine

## Summary

The binary's input dispatch, camera, initializer and day/night lighting tests stop passing for wrong implementations, and the untested dusk branches get rows. Each test is shown red against the wrong implementation it targets.

## Deliverables

- The tests below exist and each was shown red against its named wrong implementation.
- `camera` `test_create_default_camera` and `initializer` `test_build_initializes_camera` are deleted once the strengthened default-camera test lands (they only add "not identity / not zero").

## Acceptance criteria

| Test (binary crate) | Pins | Red against |
|---|---|---|
| `input_dispatcher` `test_priority_ordering` | handlers registered 0, 10, 5 run as [10, 5, 0] | `register` that appends |
| `input_dispatcher` `wheel_forwarded_when_ui_hidden`, `wheel_blocked_when_ui_visible`, `wheel_blocked_when_ui_lock_contended` (split from `wheel_forwarding_respects_ui_visibility`) | forwarded event is `MouseWheel { delta_y: 1.0 }` (use `matches!`; `InputEvent` has no `PartialEq`) | forwarding `delta_y: 0.0` |
| `initializer` `build_sets_60hz_frame_duration` (renamed from `test_build_creates_all_systems`, `is_ok` assert dropped) | frame duration 1/60 s | 1/30 s |
| `initializer` `build_applies_config_sensitivity_and_filtering` (replaces `test_build_succeeds_with_non_default_config`) | sensitivity 0.5, filtering off, `collect_mouse_delta((10,0))` → `sample_frame_input() == (5.0, 0.0)` | config ignored (gives 0.02 or 4.0) |
| `camera` `default_camera_looks_from_eye_at_center` (absorbs `test_camera_matrices_are_different`; built through `create_default_camera()`) | view maps eye to the origin and center to (0, 0, ≈-59.08) | `build()` ignoring `center` |
| `camera` `test_camera_builder_customization` | view maps the custom center to (0, 0, -150); `proj.y_axis.y ≈ 1/tan(30°)` | `with_fov_degrees` ignored |
| `frame_processor` `moon_intensity_by_time` (table) | 23.0 with the moon below the horizon → 0.0; 5.0 → 0.4; 6.0 → 0.2; dusk 19.0 → 0.2 | horizon guard deleted; dawn returns constant 0.4; dusk branch deleted |
| `frame_processor` `ambient_lighting_by_time` (table, includes the existing cases) | dusk 19.0 → colour [0.5, 0.4, 0.35], intensity 0.10 (compare within 1e-6) | dusk branch deleted |

## Tech spec

**Design:** tests in the binary crate's existing `#[cfg(test)]` modules. `InitializedApp.input_system` is public, so the config test needs no seam.

**Red proof (applies to every row):** these tests pin behaviour the code already has, so they pass on arrival. Before committing each one, apply the wrong implementation its row names as a temporary local edit, run the test, see it fail, and revert. The PR lists test → wrong implementation → failure line. A row that fails on unchanged code is a defect: stop that row, report it on this issue, and don't change production code beyond the seams named here.

**Out of scope**
- `AppConfigBuilder` deriving unset fields from supplied prefs, and the unread `AppConfig::streaming`: #187 (ENG-F22-20).
- `ChunkStreamer`: #182 (ENG-F22-15).
- `tests/event_bus_integration.rs`: #175 (ENG-F22-08).

**Test map:** the table above.

**Gate class:** glue (binary wiring and presentation, outside the domain-logic paths).

**Risks:** `forward_wheel_if_allowed` lives in `src/main.rs`, which `just mutants` excludes, so these tests are its only guard.
