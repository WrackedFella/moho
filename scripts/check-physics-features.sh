#!/usr/bin/env bash
# Fails unless rapier3d and parry3d resolve with enhanced-determinism, which keeps the
# physics step off the platform math library. Checks resolved features from
# `cargo metadata`, so a member crate overriding the workspace dependency, or a dependency
# dropping the feature, is caught. MOHO_PHYSICS_METADATA names a JSON file to read instead
# of running cargo (used by the self-tests).
set -euo pipefail

filter='
    (.packages | map({key: .id, value: .name}) | from_entries) as $name
    | (.resolve.nodes // []) as $nodes
    | ["rapier3d", "parry3d"][] as $crate
    | [$nodes[] | select($name[.id] == $crate)] as $copies
    | select(
        ($copies | length) == 0
        or any($copies[]; (.features // []) | index("enhanced-determinism") | not)
      )
    | "physics-features: \($crate) does not resolve with enhanced-determinism"'

if [[ -n "${MOHO_PHYSICS_METADATA:-}" ]]; then
    missing="$(jq -r "$filter" "$MOHO_PHYSICS_METADATA")"
else
    missing="$(cargo metadata --format-version 1 --locked | jq -r "$filter")"
fi

if [[ -n "$missing" ]]; then
    echo "$missing" >&2
    exit 1
fi
echo "physics-features: OK"
