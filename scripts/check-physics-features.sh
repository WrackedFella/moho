#!/usr/bin/env bash
# Fails unless the resolved feature set of rapier3d and parry3d includes enhanced-determinism,
# which keeps physics replays bit-identical across platforms. Reads `cargo metadata` output
# from the file named by MOHO_PHYSICS_METADATA when set (the self-test points it at a fixture),
# and runs cargo otherwise. Needs jq.
set -euo pipefail

feature="enhanced-determinism"
crates=(rapier3d parry3d)

if ! command -v jq >/dev/null; then
    echo "check-physics-features: jq not found" >&2
    exit 1
fi

if [[ -n "${MOHO_PHYSICS_METADATA:-}" ]]; then
    metadata="$(cat "$MOHO_PHYSICS_METADATA")"
else
    metadata="$(cargo metadata --format-version 1 --locked)"
fi

failing=""
for crate in "${crates[@]}"; do
    # A crate can resolve at several versions; every resolved copy must carry the feature.
    missing="$(jq -r --arg name "$crate" --arg feature "$feature" '
        (.packages | map(select(.name == $name) | .id)) as $ids
        | if ($ids | length) == 0 then "absent"
          else [.resolve.nodes[] | select(.id as $id | $ids | index($id)) | select(.features | index($feature) | not)] | length | tostring
          end' <<<"$metadata")"
    if [[ "$missing" == "absent" ]]; then
        failing+="$crate (not in the dependency graph)"$'\n'
    elif [[ "$missing" != "0" ]]; then
        failing+="$crate"$'\n'
    fi
done

if [[ -n "$failing" ]]; then
    echo "Physics crates missing the $feature feature:" >&2
    printf '%s' "$failing" >&2
    exit 1
fi
echo "physics-features: OK"
