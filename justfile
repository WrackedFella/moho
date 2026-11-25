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
check: fmt-check lint test test-doc comment-refs layering run-record-test chain-test assets-check

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
    #!/usr/bin/env bash
    set -euo pipefail
    cargo deny check
    scripts/check-physics-features.sh
    # Self-tests: the check accepts a fixture with the feature and rejects one without.
    MOHO_PHYSICS_METADATA=scripts/fixtures/physics-metadata-ok.json scripts/check-physics-features.sh
    if out=$(MOHO_PHYSICS_METADATA=scripts/fixtures/physics-metadata-missing.json scripts/check-physics-features.sh 2>&1); then
        echo "check-physics-features accepted a fixture without enhanced-determinism" >&2
        exit 1
    fi
    for crate in rapier3d parry3d; do
        grep -q "$crate" <<<"$out" || { echo "check-physics-features did not name $crate: $out" >&2; exit 1; }
    done

# LCOV coverage report at lcov.info.
cov:
    cargo llvm-cov nextest --workspace --all-features --lcov --output-path lcov.info

# Mutation-test domain code changed since branching from `base`, including uncommitted
# work. Glue (adapters, UI, windowing) is out of scope: its tests can't reach it, and the
# weekly full run covers it. The scope is CLAUDE.md's domain paths. Exit 2 means a mutant
# survived.
mutants base=base:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target
    git diff "$(git merge-base {{base}} HEAD)" > target/mutants.diff
    scope=()
    while IFS= read -r glob; do scope+=(--file "$glob"); done < <(scripts/domain-paths.sh)
    cargo mutants --workspace --all-features --in-diff target/mutants.diff "${scope[@]}"

# Full mutation run over every crate, glue included; slow, meant for scheduled CI.
mutants-full:
    cargo mutants --workspace --all-features

# Shard `k` of `n` of the full run, as the weekly CI matrix runs it. Round-robin spreads
# the slow crates across shards; the timeout floor keeps a loaded runner from reading
# slow tests as timeouts.
mutants-shard k n:
    cargo mutants --workspace --all-features --shard {{k}}/{{n}} --sharding round-robin --minimum-test-timeout 60

# Self-test of the weekly run's shard merge against fixtures: missing, unfinished,
# baseline-failed and truncated shards are reported and fail the merge.
mutants-merge-test:
    #!/usr/bin/env bash
    set -euo pipefail
    out="$(mktemp -d)"
    trap 'rm -rf "$out"' EXIT
    fixtures=scripts/fixtures/mutants-merge
    expect() {
        local count=$1 status=$2 line=$3 got rc=0
        got="$(scripts/mutants-merge.sh "$fixtures" "$count" "$out/$count")" || rc=$?
        if [[ $rc != "$status" || $got != "$line" ]]; then
            echo "mutants-merge $count shards: expected exit $status and '$line', got exit $rc and '$got'" >&2
            exit 1
        fi
    }
    expect 1 0 'caught 2  missed 1  unviable 0  timeout 1; complete 1/1; missing: none; incomplete: none'
    expect 6 1 'caught 3  missed 1  unviable 2  timeout 1; complete 2/6; missing: 4; incomplete: 1 2 3'
    diff <(printf '%s\n' 'a.rs:1:1: replace a' 'b.rs:2:2: replace b') "$out/6/missed.txt"
    echo "mutants-merge: OK"

# Self-test of the review usage records against fixtures: per-model token counts, the
# unsplit fallback, the PR comment table and the totals.
review-usage-test:
    #!/usr/bin/env bash
    set -euo pipefail
    f=scripts/fixtures/review-usage
    diff "$f/records.jsonl" <(
        PR=12 HEAD_SHA=abc TIER=full ROUND=1 STEP=review VERDICT=reject ROUTE=fix RUN_URL= \
            scripts/review-usage.sh record "$f/execution.json" | jq -c 'del(.at)'
        PR=12 HEAD_SHA=def TIER=full ROUND=1 STEP=fix VERDICT= ROUTE= RUN_URL= \
            scripts/review-usage.sh record "$f/execution-unsplit.json" | jq -c 'del(.at)')
    diff "$f/table.md" <(scripts/review-usage.sh table < "$f/records.jsonl")
    diff "$f/total.md" <(scripts/review-usage.sh total - < "$f/records.jsonl")
    echo "review-usage: OK"

# Self-test of the merge-eligibility rules: each case edits an eligible PR's facts and
# expects the decision and its exit status (0 only for `eligible`).
merge-eligibility-test:
    #!/usr/bin/env bash
    set -euo pipefail
    f=scripts/fixtures/merge-eligibility
    facts="$(mktemp)"
    trap 'rm -f "$facts"' EXIT
    while IFS=$'\t' read -r edit want; do
        [[ -z $edit || $edit == \#* ]] && continue
        jq "$edit" "$f/eligible.json" > "$facts"
        rc=0
        got="$(scripts/merge-eligibility.sh decide "$facts" | paste -sd'|' | sed 's/|/ | /g')" || rc=$?
        want_rc=1
        [[ $want == eligible* ]] && want_rc=0
        if [[ $got != "$want" || $rc != "$want_rc" ]]; then
            echo "merge-eligibility [$edit]: expected '$want', got exit $rc and '$got'" >&2
            exit 1
        fi
    done < "$f/cases.txt"
    echo "merge-eligibility: OK"

# Self-test of the chain rules: each case edits two queued items' facts and expects
# decide's lines (tabs shown as ":").
chain-test:
    #!/usr/bin/env bash
    set -euo pipefail
    f=scripts/fixtures/chain
    facts="$(mktemp)"
    trap 'rm -f "$facts"' EXIT
    while IFS=$'\t' read -r edit want; do
        [[ -z $edit || $edit == \#* ]] && continue
        jq "$edit" "$f/queued.json" > "$facts"
        got="$(scripts/chain.sh decide "$facts" | tr '\t' ':' | paste -sd'|' | sed 's/|/ | /g')"
        if [[ $got != "$want" ]]; then
            echo "chain [$edit]: expected '$want', got '$got'" >&2
            exit 1
        fi
    done < "$f/cases.txt"
    echo "chain: OK"

# Self-test of the orchestrate eval scoring against fixtures: review cause classes (a
# needs-decision outside the Blockers section doesn't count), main-loop context per call,
# and the report.
eval-score-test:
    #!/usr/bin/env bash
    set -euo pipefail
    f=scripts/fixtures/eval-score
    diff "$f/reviews.jsonl" <(for r in approve defect conflict follow-up-decision; do
        scripts/eval-score.sh review < "$f/review-$r.md"; done)
    diff "$f/context.json" <(scripts/eval-score.sh context "$f/execution.json")
    diff "$f/report.md" <(scripts/eval-score.sh report < "$f/records.jsonl")
    echo "eval-score: OK"

# Self-test of the production run record against fixtures: per-step tokens and yield,
# mutation counts, the job-summary table and the digest.
# Rerun every asset generator in place (`scripts/assets.sh`); commit what changes.
assets:
    scripts/assets.sh write

# Committed generated assets match their generators. Linux only: the committed bytes come
# from Linux, and another platform's libm may round the generators' floats differently.
# Self-test: a copy of the assets with one byte changed is rejected and named.
[linux]
assets-check:
    #!/usr/bin/env bash
    set -euo pipefail
    scripts/assets.sh check
    copy="$(mktemp -d)"
    trap 'rm -rf "$copy"' EXIT
    mkdir -p "$copy/searchlight" "$copy/flagstone"
    cp -r searchlight/assets "$copy/searchlight/"
    cp -r flagstone/assets "$copy/flagstone/"
    printf '\x00' | dd of="$copy/flagstone/assets/dig.wav" bs=1 seek=100 conv=notrunc status=none
    if out=$(MOHO_ASSETS_ROOT="$copy" scripts/assets.sh check 2>&1); then
        echo "assets check accepted a changed dig.wav" >&2
        exit 1
    fi
    grep -q 'flagstone/assets/dig.wav' <<<"$out" || { echo "assets check did not name dig.wav: $out" >&2; exit 1; }

[macos, windows]
assets-check:
    @echo "assets-check: skipped, Linux only"

run-record-test:
    #!/usr/bin/env bash
    set -euo pipefail
    f=scripts/fixtures/run-record
    diff "$f/records.jsonl" <(
        ISSUE=7 GATE_CLASS=domain HEAD_SHA=abc DEVFLOW_SHA=f20ec3e45100 RUN_URL= \
            scripts/run-record.sh record "$f/execution.json" | jq -c 'del(.at)')
    for c in heading:glue line:domain header:glue none:; do
        [[ "$(scripts/run-record.sh gate-class < "$f/gate/${c%%:*}.md")" == "${c#*:}" ]] || { echo "gate-class ${c%%:*}: expected '${c#*:}'"; exit 1; }
    done
    diff "$f/table.md" <(scripts/run-record.sh table < "$f/records.jsonl")
    diff "$f/digest.md" <(scripts/run-record.sh digest < "$f/records.jsonl")
    # Entries of a split message carry a partial output count: step outputs must still add up to the
    # result's per-model output exactly, and input and cache counts to within 20% (a real run
    # showed 2-16% unattributed, from calls the log does not hold).
    for x in execution execution-partial; do
        scripts/run-record.sh record "$f/$x.json" | jq -e '
            .models as $m | [.steps[] | .models | to_entries[]] | group_by(.key) | all(
                . as $g | $g[0].key as $k
                | ["input", "output", "cache_read", "cache_write"] | all(. as $c
                    | ($g | map(.value[if $c == "output" then "output_est" else $c end]) | add) as $s | $m[$k][$c] as $t
                    | ($s - $t | fabs) <= $t * (if $c == "output" then 0 else 0.2 end)))' > /dev/null \
            || { echo "run-record $x: step tokens do not reconcile with model totals"; exit 1; }
    done
    diff "$f/record-partial.jsonl" <(
        ISSUE=9 GATE_CLASS=glue HEAD_SHA=abc DEVFLOW_SHA=f20ec3e45100 RUN_URL= \
            scripts/run-record.sh record "$f/execution-partial.json" | jq -c 'del(.at)')
    # A result the action reports as is_error is an error record; a run cut off by the usage limit
    # names its reset, and a record with no model work stays out of the digest.
    [[ "$(scripts/run-record.sh record "$f/execution-limit.json" | jq -r .subtype)" == error ]] \
        || { echo "run-record: an is_error result must record subtype error"; exit 1; }
    [[ "$(scripts/run-record.sh limit-reset "$f/execution-limit.json")" == 3:50am ]] \
        || { echo "run-record: limit-reset must read the reset time"; exit 1; }
    [[ -z "$(scripts/run-record.sh limit-reset "$f/execution.json")" ]] \
        || { echo "run-record: a run that was not cut off has no reset time"; exit 1; }
    diff "$f/digest.md" <(cat "$f/records.jsonl" <(scripts/run-record.sh record "$f/execution-limit.json") | scripts/run-record.sh digest)
    echo "run-record: OK"

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
    rejects layering-engine-searchlight 'engine-reaches-game: moho_renderer → moho_render_api'
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
    accepts-graph layering-gpu-dev-graph layering-gpu-graph-dev-only
    rejects-graph layering-gpu-dev-graph layering-gpu-graph-dev-runtime 'gpu-at-runtime: flagstone_ui → wgpu'
    rejects-graph layering-gpu-graph layering-gpu-graph-dev-only 'gpu-outside-renderer: flagstone_ui → egui-wgpu'
    collects dirty layering-collect 'gpu-outside-renderer: flagstone_ui → wgpu'
    collects clean layering-collect 'layering: OK'
    collects devonly layering-collect 'gpu-outside-renderer: flagstone_ui → egui-wgpu'
    collects devonly layering-collect-dev 'layering: OK'

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
# moho_renderer (must pass); ui=devonly gives flagstone_ui egui-wgpu as a dev-dependency only
# (rejected by layering-collect, passed by layering-collect-dev's gpu-dev row). Expects
# 'layering: OK' with exit 0, else non-zero and the line.
[private]
_layering-collects ui table expected:
    #!/usr/bin/env bash
    set -uo pipefail
    out=$(MOHO_LAYERING_CARGO=scripts/fixtures/stub-cargo/cargo STUB_CARGO_UI={{ui}} MOHO_LAYERING_GRAPH= scripts/check-layering.sh scripts/fixtures/{{table}}.txt 2>&1)
    status=$?
    if [ '{{expected}}' = 'layering: OK' ]; then
        [ "$status" -eq 0 ] && grep -qF -- '{{expected}}' <<<"$out" && exit 0
    else
        [ "$status" -ne 0 ] && grep -qF -- '{{expected}}' <<<"$out" && exit 0
    fi
    echo "layering self-test failed: collect_graph with stub cargo, flagstone_ui {{ui}}, {{table}}" >&2
    echo "  expected: {{expected}}" >&2
    echo "  got: exit $status, output:" >&2
    sed 's/^/    /' <<<"$out" >&2
    exit 1
