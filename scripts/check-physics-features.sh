#!/usr/bin/env bash
# Fails unless every resolved version of rapier3d and parry3d has the enhanced-determinism
# feature, which makes their physics step use libm instead of the platform math library.
# A dependency edit could drop it without any test noticing. A crate
# absent from the metadata also fails. A non-empty MOHO_PHYSICS_METADATA path replaces
# `cargo metadata` as the JSON source (the self-tests use this).
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
    verdict="$(jq -r --arg name "$crate" --arg feature "$feature" '
        [.packages[] | select(.name == $name) | .id] as $ids
        | [.resolve.nodes[] | select(.id | IN($ids[])) | .features | any(. == $feature)]
        | if length == 0 then "absent" elif all then "ok" else "lacks" end' <<<"$metadata")"
    case "$verdict" in
        ok) ;;
        absent)
            echo "check-physics-features: $crate is absent from the resolved metadata" >&2
            failed=1
            ;;
        *)
            echo "check-physics-features: $crate lacks the $feature feature" >&2
            failed=1
            ;;
    esac
done

[[ $failed -eq 0 ]] || exit 1
echo "physics-features: OK"
