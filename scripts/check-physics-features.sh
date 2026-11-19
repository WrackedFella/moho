#!/usr/bin/env bash
# Fails unless every resolved rapier3d and parry3d enables `enhanced-determinism`, which
# swaps platform math for libm in the physics step (ADR-0011). Prints the crate that lost
# it. cargo-deny bans features but cannot require one, hence this script. If
# MOHO_PHYSICS_METADATA names a file, its `cargo metadata --format-version 1` JSON is read
# instead of running cargo (the self-test points it at a fixture).
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
    # One line per resolved version of the crate: "<id> <yes|no>".
    lines="$(jq -r --arg c "$crate" --arg f "$feature" '
        . as $m
        | [$m.packages[] | select(.name == $c) | .id] as $ids
        | $m.resolve.nodes[]
        | select(.id as $id | $ids | index($id))
        | "\(.id) \(if (.features | index($f)) then "yes" else "no" end)"' <<<"$metadata")"
    if [[ -z "$lines" ]]; then
        echo "check-physics-features: $crate is not in the resolved dependency graph" >&2
        failed=1
        continue
    fi
    while IFS=' ' read -r id has; do
        if [[ "$has" != yes ]]; then
            echo "check-physics-features: $crate ($id) lacks the $feature feature" >&2
            failed=1
        fi
    done <<<"$lines"
done

[[ $failed -eq 0 ]] || exit 1
echo "physics features: OK"
