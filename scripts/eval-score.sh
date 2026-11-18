#!/usr/bin/env bash
# Scoring for the orchestrate eval (.github/workflows/orchestrate-eval.yml). Every replay
# is put in one cause class, so an orchestrator defect is not counted with a planning
# conflict the orchestrator could not fix:
#   pass      gate green and the review approves
#   defect    the review rejects with no `needs-decision` blocker
#   conflict  the review rejects with a `needs-decision` blocker: the work item asks for
#             something a written rule forbids, which the plan has to settle
#   paused    the run paused with questions (a correct stop, not a failure)
#   gate-fail, no-output, unreviewed, limited (cut off by the usage limit; not scored)
# Tokens stay four counts per model, never summed across models or kinds.
#
#   eval-score.sh review < review.md               pr-review verdict -> JSON
#   eval-score.sh context <execution-file>         main-loop context per turn -> JSON
#   eval-score.sh reftests <base> <reference>      reference's new test names found in the
#                                                  checked-out tree's `cargo nextest list`
#                                                  -> JSON (run after the gate built it)
#   eval-score.sh collect <label>...               replays of one configuration: the labels
#                                                  of its runs, reruns and `-a<n>` resumes;
#                                                  the latest record of each card and run
#                                                  wins. Each is joined with every dry-run
#                                                  review of its head -> JSONL. Reads run
#                                                  logs through `gh api`; EVAL_SCORE_RUNS
#                                                  (default 30) eval runs are scanned, and
#                                                  logs are cached in EVAL_SCORE_CACHE.
#   eval-score.sh report                           JSONL from collect on stdin -> markdown
set -euo pipefail

repo() {
    if [[ -n ${GITHUB_REPOSITORY:-} ]]; then echo "$GITHUB_REPOSITORY"
    else gh repo view --json nameWithOwner --jq .nameWithOwner; fi
}

# The four verdict lines open the review; blockers are `[Xn] B<k>, ...` entries in the
# Blockers section, each running until the next entry or section.
review() {
    awk '
        BEGIN { section = ""; id = "" }
        function flush() { if (id != "") printf "B\t%s\t%s\t%d\n", id, class, nd; id = ""; class = ""; nd = 0 }
        NR <= 8 {
            for (i = 1; i <= NF; i++) {
                if ($i == "Verdict:" || $i == "Route:" || $i == "Sign-off:" || $i == "Head:" || $i == "Tier:" || $i == "Round:")
                    printf "F\t%s\t%s\n", substr($i, 1, length($i) - 1), $(i + 1)
            }
        }
        match($0, /^(#+ *)?\**(Blockers|Questions|Follow-ups|Sign-off triggers|Dropped|Not reviewed|Counts)/) {
            flush()
            section = ($0 ~ /^(#+ *)?\**Blockers/) ? "blockers" : "other"
            next
        }
        section != "blockers" { next }
        match($0, /\[X[0-9]+\]/) {
            flush()
            id = substr($0, RSTART + 1, RLENGTH - 2)
            rest = substr($0, RSTART + RLENGTH)
            class = match(rest, /B[1-6]/) ? substr(rest, RSTART, 2) : "?"
        }
        /needs-decision/ { if (id != "") nd = 1; else loose = 1 }
        END { flush(); if (loose) print "L" }
    ' | jq -Rsc '
        split("\n") | map(select(. != "") | split("\t")) as $rows
        | ($rows | map(select(.[0] == "F") | {(.[1] | ascii_downcase): .[2]}) | add // {}) as $f
        | ($rows | map(select(.[0] == "B") | {id: .[1], class: .[2], needs_decision: (.[3] == "1")})) as $b
        | ($rows | any(.[0] == "L")) as $loose
        | {verdict: $f.verdict, route: $f.route, sign_off: $f["sign-off"], head: $f.head,
           tier: $f.tier, round: ($f.round | if . == null then null else tonumber end),
           blockers: $b,
           cause: (if $f.verdict == "approve" then "pass"
                   elif $f.verdict == "reject" then
                     (if ($b | any(.needs_decision)) or $loose then "conflict" else "defect" end)
                   else "unreviewed" end)}'
}

# Context the main loop carried into each of its model calls (input + cache read + cache
# write). Subagent messages carry a parent_tool_use_id and are left out; a call split into
# several messages is counted once.
context() {
    jq -c '
        [ (if type == "array" then .[] else . end)
          | select(.type == "assistant" and (.parent_tool_use_id // null) == null and .message.usage != null)
          | {id: .message.id, u: .message.usage} ]
        | unique_by(.id)
        | map(.u | (.input_tokens // 0) + (.cache_read_input_tokens // 0) + (.cache_creation_input_tokens // 0))
        | {calls: length, peak: (max // null),
           mean: (if length == 0 then null else (add / length | floor) end)}' "$1"
}

# Test functions the reference added (`#[test]`-style attribute, then `fn name`), and
# which of them exist in the tree under test. proptest! cases carry no attribute and are
# not counted.
reftests() {
    local base=$1 reference=$2 names listed
    names=$(git diff -U0 "$base" "$reference" -- '*.rs' | awk '
        /^\+\+\+/ { next }
        /^\+[[:space:]]*#\[([a-z_]+::)*(test|rstest)[]( ]/ { pending = 3; next }
        /^\+/ && pending > 0 {
            if (match($0, /fn [A-Za-z_][A-Za-z0-9_]*/)) { print substr($0, RSTART + 3, RLENGTH - 3); pending = 0 }
            else pending--
            next
        }
        { pending = 0 }' | sort -u)
    listed=$(cargo nextest list --workspace --all-features --message-format oneline 2>/dev/null |
        awk '{ n = $NF; sub(/.*::/, "", n); print n }' | sort -u)
    jq -nc --arg names "$names" --arg listed "$listed" '
        ($names | split("\n") | map(select(. != ""))) as $want
        | ($listed | split("\n") | map(select(. != ""))) as $have
        | {total: ($want | length), present: ([$want[] | select(. as $n | $have | index($n))] | length),
           missing: [$want[] | select(. as $n | $have | index($n) | not)]}'
}

# gh api with retries: the Actions API answers 5xx under load.
gha() {
    local n
    for n in 1 2 3 4; do
        gh api "$@" && return 0
        sleep $((n * 5))
    done
    return 1
}

# Job logs, cached: a finished job's log never changes.
job_log() {
    local job=$1 cache=${EVAL_SCORE_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/eval-score}
    mkdir -p "$cache"
    [[ -s $cache/$job.log ]] || gha "repos/$(repo)/actions/jobs/$job/logs" > "$cache/$job.log"
    # Timestamps off; command echoes (ANSI-coloured) dropped.
    grep -v $'\e' "$cache/$job.log" | sed -E 's/^[0-9]{4}-[0-9-]+T[0-9:.]+Z ?//'
}

collect() {
    (($# > 0)) || { echo "eval-score: collect needs a label" >&2; exit 2; }
    local r at job log head labels runs records since
    labels=$(printf '%s\n' "$@" | jq -Rsc 'split("\n") | map(select(. != ""))')
    runs=$(gha "repos/$(repo)/actions/workflows/orchestrate-eval.yml/runs?per_page=${EVAL_SCORE_RUNS:-30}" \
        --jq '.workflow_runs[] | select(.status == "completed") | "\(.id) \(.created_at)"')
    records=$(while read -r r _; do
        [[ -n $r ]] || continue
        gha "repos/$(repo)/actions/runs/$r/jobs?per_page=100" --jq '.jobs[] | select(.name | test(" r[0-9]+$")) | .id' |
            while read -r job; do job_log "$job" | sed -n 's/^EVAL-RECORD //p'; done
    done <<<"$runs" | jq -sc --argjson labels "$labels" '
        map(select(.label as $l | $labels | any(. == $l or ($l | startswith(. + "-a")))))
        | group_by([.id, .run]) | map(max_by(.usage.at // ""))[]')
    [[ -n $records ]] || { echo "eval-score: no replay found for $*" >&2; exit 1; }
    since=$(awk '{ print $2 }' <<<"$runs" | sort | head -n1)
    # Every dry run since the oldest scanned eval run; reviews join replays on head SHA.
    gha -X GET --paginate "repos/$(repo)/actions/workflows/pr-review.yml/runs" -f event=workflow_dispatch -f per_page=100 -f created=">=$since" \
        --jq '.workflow_runs[] | select(.status == "completed") | "\(.id) \(.created_at)"' |
    while read -r r at; do
        [[ -n $r ]] || continue
        job=$(gha "repos/$(repo)/actions/runs/$r/jobs" --jq '.jobs[] | select(.name == "Review dry run") | .id')
        [[ -n $job ]] || continue
        log=$(job_log "$job")
        # The header names the reviewed range; the review's own Head line may be short.
        head=$(sed -nE 's/^### Dry run [0-9a-f]+\.\.([0-9a-f]+) against #.*/\1/p' <<<"$log" | head -n1)
        [[ -n $head ]] || continue
        awk '/^### Dry run [0-9a-f]+\.\.[0-9a-f]+ against #/ { on = 1; next } /^### Usage/ { exit } on' <<<"$log" |
            review | jq -c --arg head "$head" --arg run "$r" --arg at "$at" '. + {head: $head, run: $run, at: $at}'
    done | jq -sc --argjson recs "$(jq -sc . <<<"$records")" '
        . as $reviews
        | $recs[]
        | . as $rec
        | . + {reviews: ([$reviews[] | select(.verdict != null and .head == $rec.head)] | sort_by(.at))}'
}

report() {
    jq -rs '
        def cause:
            if .limited then "limited"
            elif .paused then "paused"
            elif (.head // "") == "" or .commits == 0 then "no-output"
            elif .gate != "pass" then "gate-fail"
            elif (.reviews | length) == 0 then "unreviewed"
            else .reviews[0].cause end;
        def fmt: if . == null then "-" else tostring end;
        def k: if . == null then "-" elif . >= 1000000 then "\(. / 100000 | round / 10)M"
               elif . >= 1000 then "\(. / 1000 | round)k" else tostring end;
        map(. + {cause: cause}) as $all
        | ($all | map(select(.cause != "limited"))) as $s
        | ($s | group_by(.id)) as $cards
        | ($s | map(select(.reviews | length > 1))) as $re
        | "### Replays", "",
          "| Card | Class | Run | Gate | Steps missing | Reviews | Cause | Blockers | Ref tests | Peak context | Turns | Min |",
          "|---|---|--:|---|---|---|---|---|--:|--:|--:|--:|",
          ($all | sort_by(.id, .run)[]
           | "| \(.id) | \(.gate_class) | \(.run) | \(.gate) | \(.steps_missing | if . == null then "-" elif length == 0 then "none" else join(",") end) | \(.reviews | map("\(.verdict)/\(.route)") | join(", ") | if . == "" then "-" else . end) | \(.cause) | \(.reviews[0].blockers // [] | map(.class + (if .needs_decision then "*" else "" end)) | join(" ") | if . == "" then "-" else . end) | \(if .reftests then "\(.reftests.present)/\(.reftests.total)" else "-" end) | \(.context.peak | k) | \(.usage.turns | fmt) | \(if .usage.duration_ms then (.usage.duration_ms / 60000 | round) else "-" end) |"),
          "", "`*` a needs-decision blocker. Cause uses the first review of the head.", "",
          "### Summary", "",
          "- Scored replays: \($s | length) (\($all | map(select(.cause == "limited")) | length) cut off by the usage limit, not scored).",
          "- Pass: \($s | map(select(.cause == "pass")) | length) of \($s | length); glue \($s | map(select(.gate_class == "glue" and .cause == "pass")) | length) of \($s | map(select(.gate_class == "glue")) | length), domain \($s | map(select(.gate_class == "domain" and .cause == "pass")) | length) of \($s | map(select(.gate_class == "domain")) | length).",
          "- By cause: \($s | group_by(.cause) | map("\(.[0].cause) \(length)") | join(", ")).",
          "- Orchestrator pass (conflicts and pauses left out): \($s | map(select(.cause == "pass")) | length) of \($s | map(select(.cause != "conflict" and .cause != "paused")) | length).",
          "- Pass on every run of a card: \($cards | map(select(length > 1 and all(.cause == "pass"))) | length) of \($cards | map(select(length > 1)) | length) cards.",
          "- Same cause on every run of a card: \($cards | map(select(length > 1 and (map(.cause) | unique | length) == 1)) | length) of \($cards | map(select(length > 1)) | length) cards.",
          "- Review flip rate (same head reviewed more than once, verdicts differ): \($re | map(select(.reviews | map(.verdict) | unique | length > 1)) | length) of \($re | length) heads.",
          "", "### Tokens per model (replays only)", "",
          "| Model | Input | Output | Cache read | Cache write |", "|---|--:|--:|--:|--:|",
          ($all | map(.usage.models // {} | to_entries[]) | group_by(.key)[]
           | "| \(.[0].key) | \(map(.value.input // 0) | add) | \(map(.value.output // 0) | add) | \(map(.value.cache_read // 0) | add) | \(map(.value.cache_write // 0) | add) |")'
}

case "${1:-}" in
    review) review ;;
    context) context "${2:?execution file}" ;;
    reftests) reftests "${2:?base}" "${3:?reference}" ;;
    collect) shift; collect "$@" ;;
    report) report ;;
    *) sed -n '2,26p' "$0" >&2; exit 2 ;;
esac
