#!/usr/bin/env bash
# Fails unless rapier3d and parry3d both resolve with the enhanced-determinism feature, which
# keeps physics results bit-identical across platforms (replays and netcode depend on it). A
# crate missing from the resolve, or resolved in several versions with any lacking the
# feature, is an offender. Reads `cargo metadata --format-version 1`, or the file named by
# MOHO_PHYSICS_METADATA when set (the self-tests point it at fixtures).
set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
    echo "check-physics-features: jq is required" >&2
    exit 1
fi

if [[ -n "${MOHO_PHYSICS_METADATA:-}" ]]; then
    if [[ ! -f "$MOHO_PHYSICS_METADATA" ]]; then
        echo "check-physics-features: metadata file '$MOHO_PHYSICS_METADATA' not found" >&2
        exit 1
    fi
    metadata="$(cat "$MOHO_PHYSICS_METADATA")"
else
    metadata="$(cargo metadata --format-version 1)"
fi

violations=""
for crate in rapier3d parry3d; do
    problem="$(jq -r --arg name "$crate" '
        [.packages[] | select(.name == $name) | .id] as $ids
        | [.resolve.nodes[] | select(.id as $id | $ids | index($id))] as $nodes
        | if ($nodes | length) == 0 then "absent from the resolve"
          elif ($nodes | all(.features | index("enhanced-determinism"))) then empty
          else "resolved without enhanced-determinism" end
    ' <<<"$metadata")"
    [[ -n "$problem" ]] && violations+="$crate: $problem"$'\n'
done

if [[ -n "$violations" ]]; then
    echo "check-physics-features: enhanced-determinism required on rapier3d and parry3d:" >&2
    printf '%s' "$violations" >&2
    exit 1
fi
echo "physics-features: OK"
