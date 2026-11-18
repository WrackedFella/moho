#!/usr/bin/env bash
# Fails unless every resolved version of rapier3d and parry3d has the enhanced-determinism
# feature, which keeps physics bit-for-bit reproducible across platforms (replays depend on
# it). Cargo unifies features, so another dependency edge could drop it silently; this reads
# the resolved features from `cargo metadata` and names each crate lacking it. A crate
# absent from the metadata also fails. If MOHO_PHYSICS_METADATA names a non-empty path, the
# metadata JSON is read from that file instead of running cargo (the self-tests use this).
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
