# The paste advisory ignore is removed

**Status:** deferred (blocked until `rapier3d`'s math stack no longer depends on `paste`)
**Feature:** [ENG-F2](_feature.md)

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
