#!/usr/bin/env bash
# Fails unless rapier3d and parry3d both resolve with the enhanced-determinism feature, so
# the physics replay stays bit-for-bit across platforms. The feature is enabled on rapier3d
# and reaches parry3d through it; a dependency change that drops it from either breaks
# determinism without failing any same-machine test. A crate missing from the resolved
# graph fails too. If MOHO_PHYSICS_METADATA names a file, `cargo metadata` JSON is read
# from it instead of running cargo (the self-tests point it at fixtures).
set -euo pipefail

if [[ -n "${MOHO_PHYSICS_METADATA:-}" ]]; then
    if [[ ! -f "$MOHO_PHYSICS_METADATA" ]]; then
        echo "check-physics-features: metadata file '$MOHO_PHYSICS_METADATA' not found" >&2
        exit 1
    fi
    metadata="$(cat "$MOHO_PHYSICS_METADATA")"
else
    metadata="$(cargo metadata --format-version 1 --locked)"
fi

violations="$(jq -r '
    . as $m
    | ["rapier3d", "parry3d"][] as $name
    | [$m.packages[] | select(.name == $name) | .id] as $ids
    | if ($ids | length) == 0 then
        "\($name): not in the resolved dependency graph"
      else
        $ids[] as $id
        | select(([$m.resolve.nodes[] | select(.id == $id) | .features[]?]
                  | index("enhanced-determinism")) == null)
        | "\($name): enhanced-determinism feature is not enabled"
      end
' <<<"$metadata")"

if [[ -n "$violations" ]]; then
    echo "Physics determinism violations:" >&2
    echo "$violations" >&2
    exit 1
fi
echo "physics-features: OK"
