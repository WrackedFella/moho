#!/usr/bin/env bash
# Fails unless the resolved features of rapier3d and parry3d include enhanced-determinism.
# Cross-platform bit-identical physics (replays, lockstep) depends on it, and Cargo unifies
# features across the workspace, so one crate dropping the feature silently breaks it for
# every consumer. Reads the resolved features from `cargo metadata` (.resolve.nodes), not the
# declared ones. If MOHO_PHYSICS_METADATA names a file, the metadata is read from it instead
# of running cargo (the self-test points it at a fixture).
set -euo pipefail

feature="enhanced-determinism"
crates=(rapier3d parry3d)

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
for crate in "${crates[@]}"; do
    # One line per resolved version of the crate: "<version> yes|no"; none when absent.
    versions="$(jq -r --arg name "$crate" --arg feature "$feature" '
        . as $m
        | [$m.packages[] | select(.name == $name)] as $pkgs
        | $pkgs[]
        | . as $p
        | "\($p.version) " + (
            if any($m.resolve.nodes[]; .id == $p.id and (.features | index($feature)))
            then "yes" else "no" end)
    ' <<<"$metadata")"
    if [[ -z "$versions" ]]; then
        violations+="$crate: not in the dependency graph"$'\n'
        continue
    fi
    while read -r version has; do
        [[ "$has" == yes ]] || violations+="$crate $version: lacks feature $feature"$'\n'
    done <<<"$versions"
done

if [[ -n "$violations" ]]; then
    echo "Physics determinism violations (Cargo.toml must enable $feature on rapier3d and parry3d):" >&2
    printf '%s' "$violations" >&2
    exit 1
fi
echo "physics-features: OK"
