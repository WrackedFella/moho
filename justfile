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
    #!/usr/bin/env bash
    set -euo pipefail
    graph="$(mktemp)"
    trap 'rm -f "$graph"' EXIT
    export MOHO_LAYERING_GRAPH="$graph"
    # Forward slashes: bash strips the backslashes of an unquoted Windows path.
    rejects() { "{{replace(just_executable(), '\', '/')}}" _layering-rejects "$@"; }
    rejects-graph() { "{{replace(just_executable(), '\', '/')}}" _layering-rejects-graph "$@"; }
    accepts-graph() { "{{replace(just_executable(), '\', '/')}}" _layering-accepts-graph "$@"; }
    collects() { "{{replace(just_executable(), '\', '/')}}" _layering-collects "$@"; }
    scripts/check-layering.sh scripts/layering.txt
    rejects layering-violation 'engine-reaches-game: flagstone_ui → flagstone_game'
    rejects layering-engine-fps 'engine-reaches-game: moho_renderer → moho_render_api'
    rejects layering-unassigned 'unassigned: moho_input'
    rejects layering-platform 'domain-reaches-platform: flagstone_ui → egui'
    rejects layering-platform 'domain-reaches-platform: flagstone → wgpu'
    rejects layering-platform 'domain-reaches-platform: flagstone_ui → winit'
    rejects layering-cross-line 'cross-game-line: flagstone_ui → flagstone_game'
    rejects layering-cross-line 'cross-game-line: flagstone → flagstone_ui'
    rejects layering-duplicate 'duplicate: moho_input'
    rejects layering-unknown 'unknown: moho_gmae'
    rejects layering-repeated-row 'repeated row: engine'
    rejects layering-gpu 'gpu-outside-renderer: moho_renderer → wgpu'
    rejects layering-gpu 'gpu-outside-renderer: moho_renderer → naga'
    rejects-graph layering-gpu-graph layering-gpu-graph-direct 'gpu-outside-renderer: flagstone_ui → wgpu'
    rejects-graph layering-gpu-graph layering-gpu-graph-egui-wgpu 'gpu-outside-renderer: flagstone_ui → egui-wgpu'
    accepts-graph layering-gpu-graph layering-gpu-graph-renderer-only
    collects dirty 'gpu-outside-renderer: flagstone_ui → wgpu'
    collects clean 'layering: OK'

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

# Like _layering-rejects, but reads the crate graph from scripts/fixtures/<graph>.graph.
[private]
_layering-rejects-graph fixture graph expected:
    #!/usr/bin/env bash
    set -uo pipefail
    out=$(MOHO_LAYERING_GRAPH=scripts/fixtures/{{graph}}.graph scripts/check-layering.sh scripts/fixtures/{{fixture}}.txt 2>&1)
    status=$?
    if [ "$status" -ne 0 ] && grep -qF -- '{{expected}}' <<<"$out"; then
        exit 0
    fi
    echo "layering self-test failed: {{graph}}" >&2
    echo "  expected: non-zero exit and line: {{expected}}" >&2
    echo "  got: exit $status, output:" >&2
    sed 's/^/    /' <<<"$out" >&2
    exit 1

# Fails unless the layering check passes on the table and graph fixture pair.
[private]
_layering-accepts-graph fixture graph:
    #!/usr/bin/env bash
    set -uo pipefail
    out=$(MOHO_LAYERING_GRAPH=scripts/fixtures/{{graph}}.graph scripts/check-layering.sh scripts/fixtures/{{fixture}}.txt 2>&1)
    status=$?
    if [ "$status" -eq 0 ] && grep -qF -- 'layering: OK' <<<"$out"; then
        exit 0
    fi
    echo "layering self-test failed: {{graph}}" >&2
    echo "  expected: exit 0 and 'layering: OK'" >&2
    echo "  got: exit $status, output:" >&2
    sed 's/^/    /' <<<"$out" >&2
    exit 1

# Runs the real graph collection against scripts/fixtures/stub-cargo/cargo, so the
# own-reach pruning in check-layering.sh is exercised. ui=dirty makes flagstone_ui's wrapper
# crate pull in wgpu (must be rejected); ui=clean leaves only flagstone's path through
# moho_renderer (must pass). Expects 'layering: OK' with exit 0, else non-zero and the line.
[private]
_layering-collects ui expected:
    #!/usr/bin/env bash
    set -uo pipefail
    out=$(MOHO_LAYERING_CARGO=scripts/fixtures/stub-cargo/cargo STUB_CARGO_UI={{ui}} MOHO_LAYERING_GRAPH= scripts/check-layering.sh scripts/fixtures/layering-collect.txt 2>&1)
    status=$?
    if [ '{{expected}}' = 'layering: OK' ]; then
        [ "$status" -eq 0 ] && grep -qF -- '{{expected}}' <<<"$out" && exit 0
    else
        [ "$status" -ne 0 ] && grep -qF -- '{{expected}}' <<<"$out" && exit 0
    fi
    echo "layering self-test failed: collect_graph with stub cargo, flagstone_ui {{ui}}" >&2
    echo "  expected: {{expected}}" >&2
    echo "  got: exit $status, output:" >&2
    sed 's/^/    /' <<<"$out" >&2
    exit 1
