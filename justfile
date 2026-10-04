# `just check` is the gate: local, CI and agents all run exactly this.

set shell := ["bash", "-euo", "pipefail", "-c"]

# Branch that diff-based checks compare against: `dev`, or the feature integration
# branch a card PR targets (`MOHO_BASE=origin/feature/<ID>-<slug>`).
base := env("MOHO_BASE", "origin/dev")

default: check

# Format the workspace in place.
fmt:
    cargo fmt --all

# Gate: formatting, lints, tests, doctests, comment refs and layering. Must pass before any commit.
check: fmt-check lint test test-doc comment-refs layering

fmt-check:
    cargo fmt --all --check

lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

[positional-arguments]
test *args:
    cargo nextest run --workspace --all-features "$@"

# nextest doesn't run doctests.
test-doc:
    cargo test --doc --workspace --all-features

# Added Rust comments must not reference work items, phases or PRs.
comment-refs base=base:
    scripts/check-comment-refs.sh {{base}}

# Licenses, advisories, banned and duplicate crates, crate sources.
deny:
    cargo deny check

# LCOV coverage report at lcov.info.
cov:
    cargo llvm-cov nextest --workspace --all-features --lcov --output-path lcov.info

# Mutation-test code changed since branching from `base`, including uncommitted
# work. Exit 2 means a mutant survived.
mutants base=base:
    mkdir -p target
    git diff "$(git merge-base {{base}} HEAD)" > target/mutants.diff
    cargo mutants --workspace --all-features --in-diff target/mutants.diff

# Full mutation run; slow, meant for scheduled CI.
mutants-full:
    cargo mutants --workspace --all-features

# Layering check on the real crate graph, plus self-tests that each rule rejects its fixture.
layering:
    scripts/check-layering.sh scripts/layering.txt
    {{just_executable()}} _layering-rejects layering-violation 'engine-reaches-game: moho_ui → moho_game'
    {{just_executable()}} _layering-rejects layering-unassigned 'unassigned: moho_types'
    {{just_executable()}} _layering-rejects layering-platform 'domain-reaches-platform: moho_ui → egui'
    {{just_executable()}} _layering-rejects layering-platform 'domain-reaches-platform: moho_ui → wgpu'
    {{just_executable()}} _layering-rejects layering-platform 'domain-reaches-platform: moho_ui → winit'
    {{just_executable()}} _layering-rejects layering-cross-line 'cross-game-line: moho_ui → moho_game'
    {{just_executable()}} _layering-rejects layering-duplicate 'duplicate: moho_types'
    {{just_executable()}} _layering-rejects layering-unknown 'unknown: moho_gmae'
    {{just_executable()}} _layering-rejects layering-repeated-row 'repeated row: engine'

# Fails unless the layering check exits non-zero on the fixture and prints the expected line.
[private]
_layering-rejects fixture expected:
    #!/usr/bin/env bash
    set -uo pipefail
    out=$(scripts/check-layering.sh scripts/fixtures/{{fixture}}.txt 2>&1)
    status=$?
    if [ "$status" -ne 0 ] && grep -qF -- '{{expected}}' <<<"$out"; then
        exit 0
    fi
    echo "layering self-test failed: {{fixture}}" >&2
    echo "  expected: non-zero exit and line: {{expected}}" >&2
    echo "  got: exit $status, output:" >&2
    sed 's/^/    /' <<<"$out" >&2
    exit 1
