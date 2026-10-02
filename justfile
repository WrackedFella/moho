# `just check` is the gate: local, CI and agents all run exactly this.

set shell := ["bash", "-euo", "pipefail", "-c"]

default: check

# Format the workspace in place.
fmt:
    cargo fmt --all

# Gate: formatting, lints, tests, doctests and comment refs. Must pass before any commit.
check: fmt-check lint test test-doc comment-refs

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
comment-refs base="origin/dev":
    scripts/check-comment-refs.sh {{base}}

# Licenses, advisories, banned and duplicate crates, crate sources.
deny:
    cargo deny check

# LCOV coverage report at lcov.info.
cov:
    cargo llvm-cov nextest --workspace --all-features --lcov --output-path lcov.info

# Mutation-test code changed since branching from `base`, including uncommitted
# work. Exit 2 means a mutant survived.
mutants base="origin/dev":
    mkdir -p target
    git diff "$(git merge-base {{base}} HEAD)" > target/mutants.diff
    cargo mutants --workspace --all-features --in-diff target/mutants.diff

# Full mutation run; slow, meant for scheduled CI.
mutants-full:
    cargo mutants --workspace --all-features
