# The paste advisory ignore is removed

**Note:** Delivered by [ENG-F2-09](ENG-F2-09-physics-stack-current.md): `rapier3d` 0.36 no longer pulls in `paste` (checked 2026-10-07), so that card removes the ignore and its PR closes this one
**Feature:** [ENG-F2](_feature.md)
**Issue:** [#97](https://github.com/WrackedFella/moho/issues/97)
**Status:** unknown
**Gate class:** unset
**Labels:** line:engine

## Summary

`paste` (RUSTSEC-2024-0436, unmaintained) is a proc-macro that reaches the
workspace through `rapier3d` → `simba` (also via `nalgebra`, `glamx` and
`parry3d`). Removing `legion` did not clear it, so `deny.toml` keeps the
ignore until a `rapier3d` release drops it.

## Deliverables

- `cargo tree --workspace -i paste` reports no match.
- The `RUSTSEC-2024-0436` ignore is removed from `deny.toml`.

## Notes

The advisory is "unmaintained", not a vulnerability, and `paste` runs only at
compile time. It isn't worth forking or patching `simba`. Unblock when a
`rapier3d` upgrade (see the `glam` watchlist entry) drops `paste`.

## Acceptance criteria

- [ ] `cargo tree --workspace -i paste` reports no match.
- [ ] `deny.toml` has no RUSTSEC-2024-0436 ignore, and `just deny` passes.
