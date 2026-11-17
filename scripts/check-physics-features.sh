#!/usr/bin/env bash
# Fails unless the resolved features of rapier3d and parry3d include enhanced-determinism,
# which keeps the physics step off the platform math library. cargo deny bans features but
# cannot require one, so this reads `cargo metadata`. If MOHO_PHYSICS_METADATA names a file,
# its contents are used instead of running cargo (the self-test points it at a fixture).
set -euo pipefail

feature="enhanced-determinism"

if [[ -n "${MOHO_PHYSICS_METADATA:-}" ]]; then
    metadata="$(cat "$MOHO_PHYSICS_METADATA")"
else
    metadata="$(cargo metadata --format-version 1)"
fi

missing=0
for crate in rapier3d parry3d; do
    # Every resolved node of the crate must carry the feature; none at all counts as missing.
    ok="$(jq -r --arg crate "$crate" --arg feature "$feature" '
        ([.packages[] | select(.name == $crate) | .id]) as $ids
        | [.resolve.nodes[] | select(.id as $id | $ids | index($id))] as $nodes
        | ($nodes | length > 0) and ($nodes | all(.features | index($feature) != null))
    ' <<<"$metadata")"
    if [[ "$ok" != "true" ]]; then
        echo "check-physics-features: $crate does not enable $feature" >&2
        missing=1
    fi
done

[[ $missing -eq 0 ]] || exit 1
echo "physics-features: OK"
