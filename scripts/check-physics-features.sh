#!/usr/bin/env bash
# Fails unless rapier3d and parry3d resolve with the enhanced-determinism feature, which
# keeps the physics step off the platform math library. Reads
# `cargo metadata` resolved features per package, so a member crate that overrides the
# workspace dependency, or a dependency that drops the feature, is caught. If
# MOHO_PHYSICS_METADATA names a file, its JSON is read instead of running cargo (the
# self-tests point it at fixtures).
set -euo pipefail

filter='
    . as $m
    | ["rapier3d", "parry3d"][] as $crate
    | ([$m.packages[] | select(.name == $crate) | .id]) as $ids
    | select(
        ($ids | length) == 0
        or ([$m.resolve.nodes[] | select(.id as $i | $ids | index($i)) | .features[]?]
            | index("enhanced-determinism") | not)
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
