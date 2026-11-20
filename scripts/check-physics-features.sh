#!/usr/bin/env bash
# Fails unless the resolved features of rapier3d and parry3d include enhanced-determinism,
# which keeps the physics step off the platform math library. `cargo deny` bans features
# but cannot require them, so the requirement is checked here against `cargo metadata`.
# If MOHO_PHYSICS_METADATA names a file, its content is read instead of running cargo (the
# self-tests point it at a fixture).
set -euo pipefail

feature="enhanced-determinism"
crates=(rapier3d parry3d)

if [[ -n "${MOHO_PHYSICS_METADATA:-}" ]]; then
    metadata="$(<"$MOHO_PHYSICS_METADATA")"
else
    metadata="$(cargo metadata --format-version 1)"
fi

failed=0
for crate in "${crates[@]}"; do
    if ! jq -e --arg crate "$crate" --arg feature "$feature" '
        [.packages[] | select(.name == $crate) | .id] as $ids
        | ($ids | length > 0)
        and any(.resolve.nodes[]; (.id as $id | $ids | index($id)) != null and (.features | index($feature)) != null)
    ' <<<"$metadata" >/dev/null; then
        echo "check-physics-features: $crate does not enable $feature" >&2
        failed=1
    fi
done
exit "$failed"
