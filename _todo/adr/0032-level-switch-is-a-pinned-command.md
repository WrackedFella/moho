# 0032 — A level switch is a command pinned to a content hash

**Status:** Accepted; narrows [0011](0011-simulation-stays-network-ready.md)

## Context

Searchlight loads test levels, including a borrowed map that exists on one machine, and
switches without restarting ([#739](https://github.com/WrackedFella/moho/issues/739)). After a
switch the simulation depends on the file's content, so a recording that stores only a level
name cannot be replayed once the file changes.

## Decision

1. A switch is a field of the game's command: the level's name and its content hash. The game
   resolves the name to a path; the engine defines no level names.
2. It applies at the start of the tick that carries it, or while paused at the boundary before
   the next tick ([0018](0018-paused-commands-apply-at-the-tick-boundary.md) rule 4). Sim state
   after the switch is a function of the prior state and the level data; the renderer
   reconciles in `frame` from the ids the sim reports.
3. The hash is SHA-256 over every source the loader reads (the file, then each buffer file in
   index order), each preceded by its length as 8 bytes little-endian. It excludes the path,
   so a copy elsewhere matches. A GLB's embedded blob is part of the file; images are not read
   by a level load and are not hashed.
4. A replay or peer whose file hashes differently refuses with an error naming the file, the
   recorded hash and the found hash. It never loads the level.
5. A restart that reloads the same level is a new switch to the same path, hashed again, so an
   edited map takes effect. Only a replay pins an old hash.

## Consequences

- A recorded session reproduces a switch exactly or says why it cannot. The netcode ADR starts
  from this: a peer without the matching file refuses.
- The hash layout is a stored contract; a change is a new hash version in the log.
- The load is not simulation state, so it may move off the main thread (ENG-F46) without
  changing the log.
- `moho_level` takes `sha2` (MIT OR Apache-2.0, pure Rust) over `blake3` (C or asm build for
  speed this load does not need) and `crc32fast` (32 bits checks integrity, not file identity).
