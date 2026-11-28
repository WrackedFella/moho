#!/usr/bin/env bash
# Usage records for the PR review workflow: what each review and fix step used, kept on
# the PR so it can be totalled without run logs. Tokens are the primary measure, kept as
# four separate counts per model (input, output, cache read, cache write) and never summed
# across models or kinds. Cost is the API-rate equivalent the run reports, shown last: the
# runs use the subscription token and are not billed per token.
#
#   review-usage.sh record <execution-file>   one JSON record on stdout. Context from the
#                                             environment: PR, HEAD_SHA, TIER, ROUND, STEP,
#                                             VERDICT, ROUTE, RUN_URL (any may be empty)
#   review-usage.sh table                     JSON records on stdin -> markdown table
#   review-usage.sh post <pr>                 JSON record on stdin -> appended to the PR's
#                                             usage comment (created on first use)
#   review-usage.sh total [pr... | -]         totals per PR, per tier and per model, from
#                                             the usage comments of the given PRs (default
#                                             the 100 most recently updated) or, with `-`,
#                                             from JSON records on stdin
#
# Only usage comments by REVIEW_USAGE_AUTHOR (default github-actions[bot]) are read: the
# repository is public, and anyone can comment.
set -euo pipefail

marker='<!-- pr-review-usage -->'
author=${REVIEW_USAGE_AUTHOR:-github-actions[bot]}

repo() {
    if [[ -n ${GITHUB_REPOSITORY:-} ]]; then echo "$GITHUB_REPOSITORY"
    else gh repo view --json nameWithOwner --jq .nameWithOwner; fi
}

# The execution file is the action's message log: an array whose `result` message carries
# turns, duration, cost and `modelUsage`, keyed by model id (subagents included).
record() {
    local file=$1
    jq -c \
        --arg pr "${PR:-}" --arg head "${HEAD_SHA:-}" --arg tier "${TIER:-}" \
        --arg round "${ROUND:-}" --arg step "${STEP:-}" --arg verdict "${VERDICT:-}" \
        --arg route "${ROUTE:-}" --arg run "${RUN_URL:-}" '
        def n: if . == null then null else tonumber end;
        def kinds: {
            input: (.inputTokens // .input_tokens),
            output: (.outputTokens // .output_tokens),
            cache_read: (.cacheReadInputTokens // .cache_read_input_tokens),
            cache_write: (.cacheCreationInputTokens // .cache_creation_input_tokens),
            cost_usd: (.costUSD // .cost_usd)
        };
        ([(if type == "array" then .[] else . end) | select(.type == "result")] | last // {}) as $r
        | ($r.modelUsage // {} | with_entries(.value |= kinds)) as $models
        | {
            v: 1,
            at: (now | todate),
            pr: ($pr | if . == "" then null else tonumber end),
            head: $head, tier: $tier, round: ($round | if . == "" then null else tonumber end),
            step: $step, verdict: $verdict, route: $route, run: $run,
            subtype: $r.subtype, turns: $r.num_turns, duration_ms: $r.duration_ms,
            models: $models,
            cost_usd: $r.total_cost_usd
          }
        # A log without per-model token counts keeps the session totals, marked unsplit,
        # so a missing field shows up instead of reading as zero.
        | if ([$models[] | .input] | all(. == null)) and $r.usage != null
          then .unsplit = ($r.usage | kinds) else . end' "$file"
}

table() {
    jq -rs '
        def fmt: if . == null then "?" else tostring end;
        def secs: if . == null then "?" else "\(. / 1000 | floor)s" end;
        def usd: if . == null then "?" else "$\(. * 100 | round / 100)" end;
        "| Step | Round | Tier | Verdict | Model | Input | Output | Cache read | Cache write | Turns | Duration | Cost (API rate) |",
        "|---|---|---|---|---|--:|--:|--:|--:|--:|--:|--:|",
        (.[] as $r
         | (($r.models | to_entries)
            + (if $r.unsplit then [{key: "unsplit (no per-model counts)", value: $r.unsplit}] else [] end)) as $rows
         | (if ($rows | length) == 0 then [{key: "?", value: {}}] else $rows end)
         | to_entries[]
         | .key as $i | .value as $m
         | "| \($r.step) | \($r.round | fmt) | \($r.tier) | \([$r.verdict, $r.route] | map(select(. != "")) | join(" / ")) | \($m.key) | \($m.value.input | fmt) | \($m.value.output | fmt) | \($m.value.cache_read | fmt) | \($m.value.cache_write | fmt) | \(if $i == 0 then ($r.turns | fmt) else "" end) | \(if $i == 0 then ($r.duration_ms | secs) else "" end) | \($m.value.cost_usd | usd) |")'
}

# Records live between `<!-- records` and `-->` in the usage comment, one JSON per line.
records_of() { awk '/^<!-- records$/ { on = 1; next } on && /^-->$/ { exit } on'; }

comment_of() {
    gh api "repos/$(repo)/issues/$1/comments" --paginate \
        --jq ".[] | select(.user.login == \"$author\" and (.body | startswith(\"$marker\"))) | {id, body}" |
        jq -sc 'first // empty'
}

post() {
    local pr=$1 new existing id records body
    new=$(jq -c .)
    existing=$(comment_of "$pr")
    [[ -n $existing ]] || existing='{}'
    id=$(jq -r '.id // empty' <<<"$existing")
    records=$( { jq -r '.body // ""' <<<"$existing" | records_of; echo "$new"; } | sed '/^$/d')
    body=$(printf '%s\n\n### Review usage\n\nTokens per model, never summed across models or kinds. Cost is the API-rate equivalent, not a bill.\n\n%s\n\n<!-- records\n%s\n-->\n' \
        "$marker" "$(table <<<"$records")" "$records")
    if [[ -n $id ]]; then
        gh api -X PATCH "repos/$(repo)/issues/comments/$id" -f body="$body" --silent
    else
        gh api "repos/$(repo)/issues/$pr/comments" -f body="$body" --silent
    fi
}

gather() {
    local prs=("$@")
    if ((${#prs[@]} == 0)); then
        mapfile -t prs < <(gh api "repos/$(repo)/pulls?state=all&sort=updated&direction=desc&per_page=100" --jq '.[].number')
    fi
    for pr in "${prs[@]}"; do
        comment_of "$pr" | jq -r '.body // ""' | records_of
    done
}

total() {
    if [[ ${1:-} == - ]]; then cat; else gather "$@"; fi | sed '/^$/d' | jq -rs '
        def kinds: ["input", "output", "cache_read", "cache_write"];
        def rows: [.[] | . as $r | ($r.models | to_entries[] | select(.value.input != null)) | {pr: $r.pr, tier: $r.tier, step: $r.step, model: .key} + .value];
        def sum(f): map(f // 0) | add;
        def report(title; keys):
            "### \(title)", "",
            "| \(keys + ["Model", "Input", "Output", "Cache read", "Cache write", "Steps", "Cost (API rate)"] | join(" | ")) |",
            "|\(keys | map("---") + ["---", "--:", "--:", "--:", "--:", "--:", "--:"] | join("|"))|",
            (group_by([(keys[] as $k | .[$k] | tostring), .model])[]
             | . as $g
             | "| \([(keys[] as $k | $g[0][$k] | tostring), $g[0].model] + (kinds | map(. as $k | $g | sum(.[$k]) | tostring)) + ["\($g | length)", "$\($g | sum(.cost_usd) * 100 | round / 100)"] | join(" | ")) |"),
            "";
        if length == 0 then "No usage records found."
        else
            (map(select(.unsplit)) | length) as $unsplit
            | rows
            | report("Per model"; []),
              report("Per tier"; ["tier"]),
              report("Per tier and step"; ["tier", "step"]),
              report("Per PR"; ["pr"]),
              (if $unsplit > 0 then "\($unsplit) record(s) had no per-model counts and are left out." else empty end)
        end'
}

case "${1:-}" in
    record) record "${2:?execution file}" ;;
    table) table ;;
    post) post "${2:?pr number}" ;;
    total) shift; total "$@" ;;
    *) sed -n '2,20p' "$0" >&2; exit 2 ;;
esac
