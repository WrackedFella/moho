# 0034 — Authoring tools outside the shipped build may be copyleft

**Status:** Accepted on merge of #824; amends [0007](0007-third-party-licence-policy.md)

## Context

Searchlight maps are built in Blender with a Searchlight add-on (GDD §5 Authoring,
[FPS-F16](https://github.com/WrackedFella/moho/issues/790)). An add-on runs inside Blender and
calls its Python API, so the add-on is a derivative work of Blender, which is
GPL-2.0-or-later. 0007 bans copyleft beyond file level outright, written when every
dependency was a crate linked into a shipped binary. The add-on is never linked into or
shipped with a game; what it produces is map data.

## Decision

1. 0007's copyleft ban covers what a distributed build contains: crates, assets and anything
   else in the shipped files. It does not cover authoring tools the team runs to make content,
   provided every rule below holds.
2. A copyleft authoring tool, and code we write for it (the Blender add-on), lives outside the
   Cargo workspace, under the line's `tools/` folder. No crate depends on it, includes its
   source or runs it at build or run time, and no shipped file contains it.
3. Code we write for a GPL host is licensed GPL-3.0-or-later and says so: the add-on's
   `blender_manifest.toml` names the licence and each Python file starts with an
   `SPDX-License-Identifier: GPL-3.0-or-later` line. It is our code, so this costs nothing
   until it is distributed.
4. The tool and the game share data, never code. A data file both read (the map component
   definition, [0035](0035-map-components-are-defined-once-as-content.md)) is our own content
   under the project's licence, like every other content file.
5. What the tool produces (glTF maps, exported meshes) is our work and ships under 0007's
   asset rule. Blender's licence places no condition on output.

## Consequences

- `just deny` is unchanged: it checks crates, and the add-on is not one.
- Giving the add-on to anyone outside the team (modders, GDD §9 Q34) distributes GPL code:
  its source goes with it under GPL-3.0-or-later. That does not reach the game, since rule 2
  keeps the two apart.
- A tool that would have to be linked into the game, or shipped beside it, is still banned by
  0007. A new copyleft authoring tool other than Blender is allowed under these rules without
  another ADR, and is named in the PR that adds it.
