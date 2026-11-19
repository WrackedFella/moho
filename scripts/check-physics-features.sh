#!/usr/bin/env bash
# enhanced-determinism keeps the physics step off the platform math library, and `cargo deny`
# cannot require a feature, hence this check. MOHO_PHYSICS_METADATA replaces the
# `cargo metadata --format-version 1` input with a file (the self-test fixtures).
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
