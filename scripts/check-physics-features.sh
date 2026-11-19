#!/usr/bin/env bash
# Fails unless both rapier3d and parry3d are built with `enhanced-determinism`, which
# physics replay and cross-platform determinism rely on. Cargo unifies features across
# the graph, so this reads the resolved features of each crate from `cargo metadata`
# rather than the manifest. A crate absent from the resolve is a failure too. If
# MOHO_PHYSICS_METADATA names a file, its JSON is read instead of running cargo (the
# justfile self-tests point it at fixtures).
set -euo pipefail

if [[ -n "${MOHO_PHYSICS_METADATA:-}" ]]; then
    if [[ ! -f "$MOHO_PHYSICS_METADATA" ]]; then
        echo "physics-features: metadata file '$MOHO_PHYSICS_METADATA' not found" >&2
        exit 1
    fi
    metadata="$(cat "$MOHO_PHYSICS_METADATA")"
else
    metadata="$(cargo metadata --format-version 1 --locked)"
fi

# One line per resolved package of the named crates: "<crate> yes|no" for the feature.
states_all="$(jq -r --args '
    . as $m
    | $ARGS.positional[] as $name
    | [$m.packages[] | select(.name == $name) | .id] as $ids
    | $m.resolve.nodes[]
    | select(.id as $id | $ids | index($id))
    | "\($name) \(if (.features // [] | index("enhanced-determinism")) then "yes" else "no" end)"
' <<<"$metadata" rapier3d parry3d)"

failed=0
for crate in rapier3d parry3d; do
    states="$(grep "^$crate " <<<"$states_all" || true)"
    if [[ -z "$states" ]]; then
        echo "physics-features: $crate is absent from the resolved dependency graph" >&2
        failed=1
    elif [[ "$states" == *no* ]]; then
        echo "physics-features: $crate is built without the enhanced-determinism feature" >&2
        failed=1
    fi
done

[[ $failed -eq 0 ]] || exit 1
echo "physics-features: OK"
