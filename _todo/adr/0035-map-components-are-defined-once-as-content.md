# 0035 — Map components are defined once, as content, and exported as named marker nodes

**Status:** Proposed

## Context

The Searchlight Blender add-on places map components (spawns, waypoints, cover and so on) as
typed objects, and the game reads them from the exported glTF
([FPS-F16](https://github.com/WrackedFella/moho/issues/790)). Two lists of components, one
in Python and one in Rust, would drift, and a drifted list exports maps the game rejects.
Today the game reads mesh-less nodes by name (`player_spawn`, `waypoint_NN`, ...) with
extras for fields, and the shipped levels are generated in that form
([0024](0024-models-are-gltf-data-placed-by-node.md) gives `moho_level` its marker type).
Blender exports with Y up and, verified on Blender 5.2.3 LTS, maps an object's local -Y (the
side Blender calls its front) to glTF local +Z, the side glTF calls the front of an asset.

## Decision

1. **One definition.** The components are listed once, in the line's content folder
   (`searchlight/content/map_components.toml`, one table per component id), in the content
   format every other definition kind uses. The add-on reads that file in place; the game
   reads it through `moho_core::content`. A feature that adds a component the game reads adds
   its entry in the same PR.
2. **What an entry says.** A label, a shape, a marker name, a numbering rule and typed fields.
   Shapes today are `point` and `oriented_point`; `volume` and fields whose choices come from
   another content kind (an anomaly's kind) are added when a feature needs them, as new
   values, without changing the existing ones.
3. **Wire format.** Each component exports as a mesh-less glTF node named by its entry: the
   marker name alone, or the marker name and `_NN` when numbered. Field values are the node's
   extras, one key per field. The add-on writes the name and extras at export through the glTF
   exporter's user-extension hook; the Blender object's own name is the designer's and never
   reaches the game. Maps made before the add-on already use this form, so both load through
   one reader.
4. **Facing.** An oriented component faces along its node's local +Z in glTF, the game's
   facing for yaw 0. In Blender that is the object's local -Y. The exporter converts one into
   the other, so neither the add-on nor the game corrects it. Roll is ignored; whether pitch
   counts is the component's to say (a camera's does, a spawn's doesn't).
5. **Rules live in the game.** Which components a map needs, which numbers may repeat and
   which field values are valid are the game's rules. The add-on only places, numbers and
   exports, and later asks the game to check (FPS-F16-03).
6. **Testing.** The game's side of the contract is tested in `just check`. The add-on is
   tested against the real Blender build it supports, the `bpy` module of the same version
   from PyPI, run headless by `just addon-test` and by a CI job when the add-on, the
   definition or the add-on's map fixture changes. That download (about 1 GB installed, one
   pinned Python) is too heavy for the gate every crate change pays.

## Consequences

- A new component is one definition entry plus the game code that gives it meaning. The panel
  offers it with no Python change.
- The definition is content, so a mod root can add components the way it adds items, once
  modding is answered (GDD §9 Q34).
- Supporting another Blender version is a new `bpy` pin and another CI row; one LTS is
  supported at a time.
- A stray mesh-less node whose name happens to match a marker (a helper empty named
  `cover_old`) is read as that component. The add-on cannot prevent it; the map check
  (FPS-F16-03) reports it.
- Renaming a marker or changing the facing rule changes every map; it needs an amendment to
  this ADR and a migration of the shipped levels.
