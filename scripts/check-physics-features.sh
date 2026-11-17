#!/usr/bin/env bash
# Fails unless the resolved features of rapier3d and parry3d include enhanced-determinism,
# which keeps rapier's step off the platform math library. cargo-deny bans features but
# cannot require them, hence this script. If MOHO_PHYSICS_METADATA names a file, it is read
# as `cargo metadata --format-version 1` output instead of running cargo (the self-test
# points it at a fixture).
set -euo pipefail

feature="enhanced-determinism"
crates=(rapier3d parry3d)

if [[ -n "${MOHO_PHYSICS_METADATA:-}" ]]; then
    metadata="$(cat "$MOHO_PHYSICS_METADATA")"
else
    metadata="$(cargo metadata --format-version 1 --locked)"
fi

failed=0
for crate in "${crates[@]}"; do
    # Resolved nodes carry package ids; the packages array maps each id to its name.
    found="$(jq -r --arg crate "$crate" --arg feature "$feature" '
        ([.packages[] | select(.name == $crate) | .id]) as $ids
        | [.resolve.nodes[] | select(.id as $id | $ids | index($id)) | select(.features | index($feature))]
        | length' <<<"$metadata")"
    if [[ "$found" -eq 0 ]]; then
        echo "check-physics-features: $crate does not enable $feature" >&2
        failed=1
    fi
done

[[ "$failed" -eq 0 ]] || exit 1
echo "physics-features: OK"
