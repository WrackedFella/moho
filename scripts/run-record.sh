#!/usr/bin/env bash
# Production run records: what each `claude.yml` orchestrate run used and which steps
# yielded anything, computed in the job from the action's execution log and printed as
# one `RUN-RECORD {json}` line. The log itself is never uploaded: Actions masks secrets in
# job logs but not in artifacts, and the repository is public. Tokens are four counts per
# model, never summed across models or kinds; cost is the API-rate equivalent, last.
#
# A step is the subagent or skill the main loop called (`devflow:builder (tests)`,
# `skill devflow:wiki`); `orchestrator` is the main loop itself. Subagent messages carry a
# parent_tool_use_id that leads to the call that started them. Yield is what a step left
# Per-step output tokens are the result's per-model output shared across that model's steps by
# the size of what each step wrote (the log's own per-message counts are partial); input and
# cache counts are exact per message. Yield is what a step left
# behind that the log shows: files it edited (and how many were test files), and the
# mutation counts the main loop read. Inline unit tests in a source file count as source.
#
#   run-record.sh record <execution-file>   one JSON record on stdout. Context from the
#                                           environment: ISSUE, GATE_CLASS, HEAD_SHA,
#                                           DEVFLOW_SHA (default: read from the plugin
#                                           install), RUN_URL (any may be empty)
#   run-record.sh gate-class                issue body on stdin -> its gate class, or nothing
#   run-record.sh table                     records on stdin -> markdown, for the job summary
#   run-record.sh digest                    records on stdin -> markdown: per gate class,
#                                           runs and tokens per model, and per step the runs
#                                           that used it and the runs where it left nothing
#   run-record.sh collect                   RUN-RECORD lines of recent `claude.yml` runs
#                                           (RUN_RECORD_RUNS, default 50) -> JSONL. Logs are
#                                           cached in RUN_RECORD_CACHE.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)

repo() {
    if [[ -n ${GITHUB_REPOSITORY:-} ]]; then echo "$GITHUB_REPOSITORY"
    else gh repo view --json nameWithOwner --jq .nameWithOwner; fi
}

# The commit of the devflow plugin the run loaded: the install record names it, and the
# versioned cache directory is the fallback.
devflow_sha() {
    local f="$HOME/.claude/plugins/installed_plugins.json" sha=""
    if [[ -f $f ]]; then
        sha=$(jq -r '[.plugins["devflow@claude-skills"][0] | .gitCommitSha, .version] | map(select(. != null)) | first // empty' "$f" 2>/dev/null || true)
    fi
    if [[ -z $sha ]]; then
        sha=$(find "$HOME/.claude/plugins/cache/claude-skills/devflow" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' 2>/dev/null | head -n1 || true)
    fi
    echo "$sha"
}

# Per-step tokens and yield from the message log (an array, or JSONL slurped).
steps_of() {
    jq -c '
        def msgs: if type == "array" then .[] else . end;
        [msgs] as $all
        # Every tool call, keyed by id, with the call that started the message it sits in.
        | ([ $all[] | select(.type == "assistant") | .parent_tool_use_id as $p
             | .message.content[]? | select(.type == "tool_use")
             | {key: .id, value: {name, input, parent: $p}} ] | from_entries) as $calls
        | def lbl($c):
            if $c.name == "Agent" or $c.name == "Task" then
              ($c.input.subagent_type // "general")
              + (($c.input.prompt // "" | capture("(?m)^\\s*Mode:\\s*(?<m>[A-Za-z-]+)") | " (\(.m | ascii_downcase))") // "")
            elif $c.name == "Skill" then "skill \($c.input.skill // $c.input.command // "?")"
            else $c.name end;
          def root($p): if $p == null then "orchestrator"
            elif ($calls[$p] // null) == null then "unknown"
            elif $calls[$p].parent == null then lbl($calls[$p])
            else root($calls[$p].parent) end;
        # A message split over several log entries repeats its usage: count each message once.
        # Input and cache counts are complete in every entry; the output count is only what the
        # stream held when the entry was written, a few tokens (about 1/25 of the real total on
        # run 45474158961), and the log carries no later correction per message. The `result`
        # entry has the true output per model, so that total is shared across the steps of that model
        # in proportion to the size of what each step wrote (text, thinking, tool input).
        def size: [.[]? | (.text // .thinking // (.input | tojson) // "") | length / 4 + 10] | add // 0;
        def alloc($t): (add // 0) as $w
            | if $w == 0 then null else
              map(. * $t / $w) as $x | ($x | map(floor)) as $f | ($t - ($f | add)) as $r
              | ([range(0; length)] | sort_by(-($x[.] - $f[.]), .) | .[:$r]) as $up
              | [range(0; length) | . as $i | $f[$i] + (if $up | index($i) then 1 else 0 end)] end;
        ([ $all[] | select(.type == "result") ] | last // {} | .modelUsage // {}
         | with_entries(.value |= (.outputTokens // .output_tokens))) as $truth
        | [ $all[] | select(.type == "assistant" and .message.usage != null)
          | {id: (.message.id // ""), step: root(.parent_tool_use_id), model: (.message.model // "unknown"),
             u: .message.usage, w: (.message.content | size)}
        ] as $turns
        | ($turns | map(select(.id != "")) | group_by(.id)
           | map(.[0] + {u: (max_by(.u.output_tokens // 0) | .u), w: (map(.w) | add)})) as $counted
        | def edits($step): [ $all[] | select(.type == "assistant")
              | select(root(.parent_tool_use_id) == $step)
              | .message.content[]? | select(.type == "tool_use" and (.name | IN("Edit", "Write", "MultiEdit", "NotebookEdit")))
              | {path: (.input.file_path // .input.notebook_path // ""),
                 text: ([.input.new_string, .input.content, (.input.edits[]?.new_string)] | map(select(. != null)) | join("\n"))} ]
            | map(select(.path != "")) | group_by(.path) | map({path: .[0].path, text: (map(.text) | join("\n"))});
          # A test file by path, or any file an edit added a Rust test to (tests are inline modules).
          def is_test: (.path | test("(^|/)tests?/|_tests?\\.rs$|(^|/)tests?\\.rs$"))
            or (.text | test("#\\[(cfg\\(test\\)|test|tokio::test)\\]"));
          # One row per step and model, then the true output of each model shared across its rows.
          ($counted | group_by([.step, .model]) | map({
              step: .[0].step, model: .[0].model, calls: length, w: (map(.w) | add),
              input: (map(.u.input_tokens // 0) | add),
              output: (map(.u.output_tokens // 0) | add),
              cache_read: (map(.u.cache_read_input_tokens // 0) | add),
              cache_write: (map(.u.cache_creation_input_tokens // 0) | add)})) as $rows
        | ($rows | group_by(.model) | map(
              (.[0].model) as $m
              | (if $truth[$m] == null then null else (map(.w) | alloc($truth[$m])) end) as $out
              | to_entries | map(.value + (if $out == null then {} else {output: $out[.key]} end)))
              | add // []) as $shared
        | ($shared | group_by(.step) | map({
              step: .[0].step,
              calls: (map(.calls) | add),
              models: (map({key: .model, value: {input, output, cache_read, cache_write}}) | from_entries)
            })) as $steps
        | ([ $all[] | select(.type == "assistant" and .parent_tool_use_id == null)
             | .message.content[]? | select(.type == "tool_use" and .name == "Bash")
             | {key: .id, value: (.input.command // "")} ] | from_entries) as $cmds
        # The last mutation run the main loop read.
        | ([ $all[] | select(.type == "user" and .parent_tool_use_id == null)
             | .message.content[]? | select(type == "object" and .type == "tool_result")
             | select(($cmds[.tool_use_id] // "") | test("(^|[ /;&|])(just mutants|cargo mutants)"))
             | (.content | if type == "string" then . else (map(.text // "") | join("\n")) end) ]
           | map(select(test("[0-9]+ (caught|missed)"))) | last // null) as $mut
        | {
            steps: ($steps | map(. + {edits: (edits(.step) as $e | {files: ($e | length), tests: ($e | map(select(is_test)) | length)})})
                    | sort_by(.step)),
            mutants: (if $mut == null then null else
                {caught: (($mut | capture("(?<n>[0-9]+) caught").n // "0") | tonumber),
                 missed: (($mut | capture("(?<n>[0-9]+) missed").n // "0") | tonumber),
                 unviable: (($mut | capture("(?<n>[0-9]+) unviable").n // "0") | tonumber),
                 timeout: (($mut | capture("(?<n>[0-9]+) timeout").n // "0") | tonumber)} end)
          }' "$1"
}

# The gate class an issue body declares: a `gate=` board header, a `Gate class:` line, or the
# value under a `### Gate class` heading (backticks and a full stop allowed).
gate_class() {
    awk '
        match($0, /gate=[a-z]+/) { print substr($0, RSTART + 5, RLENGTH - 5); exit }
        match(tolower($0), /^gate class: *[a-z]+/) { v = substr(tolower($0), RSTART, RLENGTH); sub(/^gate class: */, "", v); print v; exit }
        tolower($0) ~ /^#+ *gate class *$/ { want = 1; next }
        want && /[^[:space:]]/ { if (match(tolower($0), /[a-z]+/)) { print substr(tolower($0), RSTART, RLENGTH); exit } want = 0 }
    '
}

record() {
    local file=$1 usage steps
    usage=$(STEP=orchestrate PR="" "$here/review-usage.sh" record "$file")
    steps=$(steps_of "$file")
    jq -c --arg issue "${ISSUE:-}" --arg gate_class "${GATE_CLASS:-}" --arg head "${HEAD_SHA:-}" \
        --arg devflow "${DEVFLOW_SHA:-$(devflow_sha)}" --arg run "${RUN_URL:-}" --argjson extra "$steps" '
        {v: 1, at, issue: ($issue | if . == "" then null else tonumber end),
         gate_class: (if $gate_class == "" then null else $gate_class end),
         head: (if $head == "" then null else $head end),
         devflow: (if $devflow == "" then null else $devflow end),
         run: (if $run == "" then null else $run end),
         subtype, turns, duration_ms, models, cost_usd}
        + (if .unsplit then {unsplit} else {} end)
        + $extra' <<<"$usage"
}

table() {
    jq -rs '
        def fmt: if . == null then "?" else tostring end;
        def secs: if . == null then "?" else "\(. / 1000 | floor)s" end;
        def usd: if . == null then "?" else "$\(. * 100 | round / 100)" end;
        .[] as $r
        | "#### Run record: #\($r.issue | fmt) (\($r.gate_class // "?")), devflow \($r.devflow // "?")", "",
          "\($r.subtype // "?"), \($r.turns | fmt) turns, \($r.duration_ms | secs), cost \($r.cost_usd | usd) at API rates. Tokens per model, never summed:", "",
          "| Step | Calls | Model | Input | Output | Cache read | Cache write | Files edited | Test files |",
          "|---|--:|---|--:|--:|--:|--:|--:|--:|",
          ($r.steps[] as $s | ($s.models | to_entries) as $m
           | $m | to_entries[] | .key as $i | .value as $e
           | "| \(if $i == 0 then $s.step else "" end) | \(if $i == 0 then $s.calls else "" end) | \($e.key) | \($e.value.input) | \($e.value.output) | \($e.value.cache_read) | \($e.value.cache_write) | \(if $i == 0 then $s.edits.files else "" end) | \(if $i == 0 then $s.edits.tests else "" end) |"),
          "",
          (if $r.mutants then "Mutants: \($r.mutants.caught) caught, \($r.mutants.missed) missed, \($r.mutants.unviable) unviable, \($r.mutants.timeout) timeout." else "Mutants: no mutation run read." end)'
}

digest() {
    jq -rs '
        def k: if . == null then "-" elif . >= 1000000 then "\(. / 100000 | round / 10)M"
               elif . >= 1000 then "\(. / 1000 | round)k" else tostring end;
        def median: sort | if length == 0 then null else .[(length - 1) / 2 | floor] end;
        if length == 0 then "No run records." else
        group_by(.gate_class // "unknown")[] as $g
        | "### \($g[0].gate_class // "unknown"): \($g | length) runs", "",
          "Median tokens per run and model (never summed across models or kinds):", "",
          "| Model | Input | Output | Cache read | Cache write |", "|---|--:|--:|--:|--:|",
          ([$g[] | .models | to_entries[]] | group_by(.key)[]
           | "| \(.[0].key) | \(map(.value.input) | median | k) | \(map(.value.output) | median | k) | \(map(.value.cache_read) | median | k) | \(map(.value.cache_write) | median | k) |"),
          "", "Steps: runs that called the step, and of those the runs where it edited no file.", "",
          "| Step | Runs | Left nothing | Median calls |", "|---|--:|--:|--:|",
          ([$g[] | .steps[]] | group_by(.step)[]
           | "| \(.[0].step) | \(length) | \(if .[0].step == "orchestrator" then "-" else map(select(.edits.files == 0)) | length end) | \(map(.calls) | median) |"),
          "", "Mutation: \([$g[] | select(.mutants != null)] | length) of \($g | length) runs read a result; survivors in \([$g[] | select(.mutants != null and .mutants.missed > 0)] | length).", ""
        end'
}

gha() {
    local n
    for n in 1 2 3 4; do gh api "$@" && return 0; sleep $((n * 5)); done
    return 1
}

collect() {
    local cache=${RUN_RECORD_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/run-record} job
    mkdir -p "$cache"
    gha "repos/$(repo)/actions/workflows/claude.yml/runs?per_page=${RUN_RECORD_RUNS:-50}" \
        --jq '.workflow_runs[] | select(.status == "completed") | .id' |
    while read -r r; do
        [[ -n $r ]] || continue
        gha "repos/$(repo)/actions/runs/$r/jobs?per_page=100" --jq '.jobs[] | select(.name == "Orchestrate") | .id' |
        while read -r job; do
            [[ -s $cache/$job.log ]] || gha "repos/$(repo)/actions/jobs/$job/logs" > "$cache/$job.log"
            sed -E 's/^[0-9]{4}-[0-9-]+T[0-9:.]+Z ?//' "$cache/$job.log" | sed -n 's/^RUN-RECORD //p'
        done
    done
}

case "${1:-}" in
    record) record "${2:?execution file}" ;;
    gate-class) gate_class ;;
    table) table ;;
    digest) digest ;;
    collect) collect ;;
    *) sed -n '2,26p' "$0" >&2; exit 2 ;;
esac
