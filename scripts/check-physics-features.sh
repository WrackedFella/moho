#!/usr/bin/env bash
# Fails unless the resolved features of rapier3d and parry3d include enhanced-determinism.
# The feature makes rapier, parry, nalgebra and (through parry's glamx) glam use libm
# instead of the platform math library, so a physics step's results do not depend on the
# host's libm; without it a recorded run can stop replaying on another OS. The resolved
# graph is checked, not the manifest, so a dependency edit that drops the feature fails
# here. `cargo metadata` unifies features across the workspace, so a feature enabled
# only by another member still passes. If MOHO_PHYSICS_METADATA names a file, the
# `cargo metadata` JSON is read from it (the self-tests point it at fixtures).
set -euo pipefail

feature="enhanced-determinism"
crates=(rapier3d parry3d)

if ! command -v jq >/dev/null 2>&1; then
    echo "check-physics-features: jq not found" >&2
    exit 1
fi

source_file="${MOHO_PHYSICS_METADATA:-}"
if [[ -n "$source_file" ]]; then
    if [[ ! -f "$source_file" ]]; then
        echo "check-physics-features: metadata file '$source_file' not found" >&2
        exit 1
    fi
    metadata="$(<"$source_file")"
else
    metadata="$(cargo metadata --format-version 1)" || {
        echo "check-physics-features: cargo metadata failed" >&2
        exit 1
    }
fi

failed=0
for crate in "${crates[@]}"; do
    # One line per resolved package of that name: "true" or "false" for the feature.
    results="$(jq -r --arg name "$crate" --arg feature "$feature" '
        [.packages[] | select(.name == $name) | .id] as $ids
        | .resolve.nodes[] | select(.id as $id | $ids | index($id))
        | (.features | index($feature)) != null' <<<"$metadata")" || {
        echo "check-physics-features: cannot read metadata" >&2
        exit 1
    }
    if [[ -z "$results" ]]; then
        echo "$crate: not in the resolved dependency graph" >&2
        failed=1
    elif [[ "$results" == *false* ]]; then
        echo "$crate: $feature not enabled" >&2
        failed=1
    fi
done

[[ $failed -eq 0 ]] || exit 1
echo "physics-features: OK"
