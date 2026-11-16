#!/usr/bin/env bash
# Fails unless the resolved features of rapier3d and parry3d include enhanced-determinism,
# which keeps the physics step off the platform math library. cargo-deny bans features but
# cannot require them, hence this script. If MOHO_PHYSICS_METADATA names a file, it is read
# as `cargo metadata --format-version 1` output instead of running cargo (the self-tests
# point it at fixtures).
set -euo pipefail

feature="enhanced-determinism"
if [[ -n "${MOHO_PHYSICS_METADATA:-}" ]]; then
    metadata="$(cat "$MOHO_PHYSICS_METADATA")"
else
    metadata="$(cargo metadata --format-version 1)"
fi

failed=""
for crate in rapier3d parry3d; do
    if ! jq -e --arg crate "$crate" --arg feature "$feature" '
        ([.packages[] | select(.name == $crate) | .id]) as $ids
        | ($ids | length > 0)
          and any(.resolve.nodes[]; (.id as $id | $ids | index($id) != null) and (.features | index($feature) != null))
    ' <<<"$metadata" >/dev/null; then
        echo "check-physics-features: $crate does not enable $feature" >&2
        failed=1
    fi
done

[[ -z "$failed" ]] || exit 1
echo "physics features: OK"
