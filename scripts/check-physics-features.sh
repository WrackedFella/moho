#!/usr/bin/env bash
# Fails unless rapier3d and parry3d both resolve with `enhanced-determinism` in the default
# workspace resolution (no --all-features: the check must see what the build uses). Without it
# the physics step is not bit-identical across platforms. Names every crate lacking the
# feature, and fails if a crate is absent from the resolve graph. If MOHO_PHYSICS_METADATA
# names a file, the `cargo metadata` JSON is read from it (the self-tests point it at
# fixtures).
set -euo pipefail

feature="enhanced-determinism"
crates=(rapier3d parry3d)

if [[ -n "${MOHO_PHYSICS_METADATA:-}" ]]; then
    metadata="$(cat "$MOHO_PHYSICS_METADATA")"
else
    metadata="$(cargo metadata --format-version 1)"
fi

failed=0
for crate in "${crates[@]}"; do
    lines="$(jq -r --arg name "$crate" --arg feature "$feature" '
        (.packages | map({key: .id, value: .name}) | from_entries) as $names
        | .resolve.nodes[]
        | select($names[.id] == $name)
        | if (.features | index($feature)) then "ok" else "missing" end
    ' <<<"$metadata")"
    if [[ -z "$lines" ]]; then
        echo "physics-features: $crate is not in the resolved workspace" >&2
        failed=1
    elif grep -qx missing <<<"$lines"; then
        echo "physics-features: $crate lacks the $feature feature" >&2
        failed=1
    fi
done

[[ $failed -eq 0 ]] || exit 1
echo "physics-features: OK"
