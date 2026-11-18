#!/usr/bin/env bash
# Fails unless the resolved rapier3d and parry3d both build with `enhanced-determinism`, so
# physics results stay bit-identical across platforms. Reads `cargo metadata` output; if
# MOHO_PHYSICS_METADATA names a file, it is read instead of running cargo (the self-tests
# point it at fixtures). A crate missing from the resolve graph fails too.
set -euo pipefail

feature="enhanced-determinism"

if [[ -n "${MOHO_PHYSICS_METADATA:-}" ]]; then
    metadata="$(cat "$MOHO_PHYSICS_METADATA")"
else
    metadata="$(cargo metadata --format-version 1)"
fi

status=0
for crate in rapier3d parry3d; do
    # Every resolve node of the crate, one per resolved version; each must carry the feature.
    result="$(jq -r --arg crate "$crate" --arg feature "$feature" '
        [.packages[] | select(.name == $crate) | .id] as $ids
        | [.resolve.nodes[] | select(.id as $id | $ids | index($id))] as $nodes
        | if ($nodes | length) == 0 then "absent"
          elif ($nodes | all(.features | index($feature))) then "ok"
          else "lacks" end
    ' <<<"$metadata")"
    case "$result" in
        ok) ;;
        absent) echo "check-physics-features: $crate not found in the resolve graph" >&2; status=1 ;;
        *) echo "check-physics-features: $crate lacks $feature" >&2; status=1 ;;
    esac
done

[[ $status -eq 0 ]] && echo "physics-features: OK"
exit "$status"
