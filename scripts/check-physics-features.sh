#!/usr/bin/env bash
# Fails when rapier3d or parry3d is resolved without its enhanced-determinism feature, or is
# absent from the resolve. The feature keeps floating-point results bit-identical across
# platforms, which replays and lockstep rely on; cargo feature unification can silently drop
# it if a dependency edge changes. If MOHO_PHYSICS_METADATA names a file, the cargo metadata
# JSON is read from it instead of running cargo (the self-tests point it at fixtures).
set -euo pipefail

if [[ -n "${MOHO_PHYSICS_METADATA:-}" ]]; then
    metadata="$(cat "$MOHO_PHYSICS_METADATA")"
else
    metadata="$(cargo metadata --format-version 1)"
fi

# Every resolved version of the crate must enable the feature; a crate with no resolved node lacks it.
lacking="$(jq -r '
    . as $m
    | ["rapier3d", "parry3d"][] as $name
    | [ $m.resolve.nodes[] as $node
        | $m.packages[] | select(.id == $node.id and .name == $name)
        | ($node.features | index("enhanced-determinism")) != null ] as $found
    | select(($found | length) == 0 or ($found | all | not))
    | $name
' <<<"$metadata")"

if [[ -n "$lacking" ]]; then
    while IFS= read -r crate; do
        echo "physics-features: $crate lacks enhanced-determinism" >&2
    done <<<"$lacking"
    exit 1
fi
echo "physics-features: OK"
