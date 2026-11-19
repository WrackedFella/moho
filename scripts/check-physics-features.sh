#!/usr/bin/env bash
# Fails unless the resolved features of rapier3d and parry3d include enhanced-determinism,
# which keeps the physics step off the platform math library. `cargo deny` bans features
# but cannot require them, hence this check. MOHO_PHYSICS_METADATA names a file holding
# `cargo metadata --format-version 1` output; the self-tests point it at fixtures.
set -euo pipefail

feature="enhanced-determinism"
metadata="${MOHO_PHYSICS_METADATA:-}"
if [[ -z "$metadata" ]]; then
    metadata="$(mktemp)"
    trap 'rm -f "$metadata"' EXIT
    cargo metadata --format-version 1 --locked >"$metadata"
fi

status=0
for crate in rapier3d parry3d; do
    if ! jq -e --arg crate "$crate" --arg feature "$feature" '
        [.resolve.nodes[] | select(.id | contains("#" + $crate + "@")) | .features | index($feature)]
        | length > 0 and all(. != null)' "$metadata" >/dev/null; then
        echo "check-physics-features: $crate does not enable $feature" >&2
        status=1
    fi
done
[[ $status -eq 0 ]] && echo "physics-features: OK"
exit $status
