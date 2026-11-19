#!/usr/bin/env bash
# Fails unless every resolved rapier3d and parry3d enables `enhanced-determinism`, which
# swaps platform math for libm in the physics step (ADR-0011). cargo-deny can ban a
# feature but not require one, hence this script. MOHO_PHYSICS_METADATA, if set, names a
# `cargo metadata` JSON file read instead of running cargo (the self-test's fixture).
set -euo pipefail

feature="enhanced-determinism"
crates=(rapier3d parry3d)

if [[ -n "${MOHO_PHYSICS_METADATA:-}" ]]; then
    metadata="$(cat "$MOHO_PHYSICS_METADATA")"
else
    metadata="$(cargo metadata --format-version 1 --locked)"
fi

# One line per resolved version of each crate: "<crate> <id> <yes|no>".
crates_json="$(printf '"%s",' "${crates[@]}")"
lines="$(jq -r --argjson crates "[${crates_json%,}]" --arg f "$feature" '
    . as $m
    | $crates[] as $c
    | [$m.packages[] | select(.name == $c) | .id] as $ids
    | $m.resolve.nodes[]
    | select(.id as $id | $ids | index($id))
    | "\($c) \(.id) \(if (.features | index($f)) then "yes" else "no" end)"' <<<"$metadata")"

failed=0
for crate in "${crates[@]}"; do
    if ! grep -q "^$crate " <<<"$lines"; then
        echo "check-physics-features: $crate is not in the resolved dependency graph" >&2
        failed=1
    fi
done
while IFS=' ' read -r crate id has; do
    if [[ -n "$crate" && "$has" != yes ]]; then
        echo "check-physics-features: $crate ($id) lacks the $feature feature" >&2
        failed=1
    fi
done <<<"$lines"

[[ $failed -eq 0 ]] || exit 1
echo "physics features: OK"
