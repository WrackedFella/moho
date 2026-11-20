#!/usr/bin/env bash
# Fails unless rapier3d and parry3d resolve with the enhanced-determinism feature, which
# keeps the physics step off the platform math library. Reads
# `cargo metadata` resolved features per package, so a member crate that overrides the
# workspace dependency, or a dependency that drops the feature, is caught. If
# MOHO_PHYSICS_METADATA names a file, its JSON is read instead of running cargo (the
# self-tests point it at fixtures).
set -euo pipefail

if [[ -n "${MOHO_PHYSICS_METADATA:-}" ]]; then
    if [[ ! -f "$MOHO_PHYSICS_METADATA" ]]; then
        echo "check-physics-features: metadata file '$MOHO_PHYSICS_METADATA' not found" >&2
        exit 1
    fi
    metadata="$(cat "$MOHO_PHYSICS_METADATA")"
else
    metadata="$(cargo metadata --format-version 1 --locked)"
fi

missing="$(jq -r '
    . as $m
    | ["rapier3d", "parry3d"][] as $crate
    | ([$m.packages[] | select(.name == $crate) | .id]) as $ids
    | select(
        ($ids | length) == 0
        or ([$m.resolve.nodes[] | select(.id as $i | $ids | index($i)) | .features[]?]
            | index("enhanced-determinism") | not)
      )
    | $crate' <<<"$metadata")"

if [[ -n "$missing" ]]; then
    while IFS= read -r crate; do
        echo "physics-features: $crate does not resolve with enhanced-determinism" >&2
    done <<<"$missing"
    exit 1
fi
echo "physics-features: OK"
