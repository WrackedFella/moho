#!/usr/bin/env bash
# Decides which queued work items start next, for chain.yml (D0027-D0030 in claude-skills).
# A work item labelled `agent-queued` is approved and waits for every item blocking it
# (`blocked by`) to close; this script picks the ones that may start now.
#
#   chain.sh gather            prints the facts as JSON (needs gh, GH_REPO; AGENT_MAX_RUNS)
#   chain.sh decide <facts>    prints one tab-separated line per queued item, oldest first:
#                                start   <n>  -      <why>
#                                wait    <n>  <key>  <why>
#                                comment <n>  <key>  <text to post once>
#
# `decide` is pure, so the rules are tested against fixtures (`just chain-test`). A
# `comment` line is printed only while the item has no comment carrying the key's marker
# (`<!-- chain: <key> -->`), so each stop is reported once.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

QUEUE=agent-queued

gather() {
    : "${GH_REPO:?}"
    local items='[]' n issue blockers markers active
    for n in $(gh api "repos/$GH_REPO/issues?labels=$QUEUE&state=open&per_page=100" --paginate \
        --jq '.[] | select(.pull_request | not) | .number'); do
        issue=$(gh api "repos/$GH_REPO/issues/$n")
        blockers=$(gh api "repos/$GH_REPO/issues/$n/dependencies/blocked_by" --paginate \
            --jq '.[] | {number, state, state_reason}' | jq -s .)
        markers=$(gh api "repos/$GH_REPO/issues/$n/comments" --paginate \
            --jq '.[].body | scan("<!-- chain: ([a-z-]+) -->") | .[0]' | jq -R . | jq -s 'unique')
        items=$(jq --argjson i "$issue" --argjson b "$blockers" --argjson m "$markers" '. + [{
            number: $i.number,
            labels: [$i.labels[].name],
            eligible: ((($i.body // "") | capture("<!-- board:[^>]*\\beligible=(?<e>[a-z]+)") | .e) // null),
            open_blockers: $i.issue_dependencies_summary.blocked_by,
            blockers: $b,
            markers: $m
        }]' <<<"$items")
    done
    # Unattended runs in flight: card runs (a run sleeping for the usage-limit reset
    # counts) and eval replays, which share the subscription's usage window.
    active=0
    for wf in claude.yml orchestrate-eval.yml; do
        for status in queued in_progress; do
            active=$((active + $(gh api "repos/$GH_REPO/actions/workflows/$wf/runs?status=$status&per_page=1" --jq .total_count)))
        done
    done
    jq -n --argjson items "$items" --argjson active "$active" --arg cap "${AGENT_MAX_RUNS:-1}" \
        '{cap: ($cap | tonumber? // 1), active: $active, items: $items}'
}

decide() {
    jq -r '
        def line($a; $n; $k; $why): [$a, ($n | tostring), $k, $why] | join("\t");
        # A stop is reported once: a comment while the marker is missing, then a wait.
        def stop($n; $markers; $k; $text):
            if ($markers | index($k)) then line("wait"; $n; $k; "reported") else line("comment"; $n; $k; $text) end;
        .cap as $cap
        | ([$cap - .active, 0] | max) as $free
        | reduce (.items | sort_by(.number))[] as $i ({out: [], free: $free};
            ($i.blockers | map(select(.state == "closed" and .state_reason != "completed")) | map("#\(.number)")) as $dropped
            | if ($i.labels | index("agent-ready")) then .out += [line("wait"; $i.number; "running"; "already agent-ready")]
              elif ($i.labels | index("agent-paused")) then .out += [line("wait"; $i.number; "paused"; "labelled agent-paused")]
              elif ($i.labels | index("needs-human")) then .out += [line("wait"; $i.number; "needs-human"; "labelled needs-human")]
              elif $i.eligible == "no" then .out += [stop($i.number; $i.markers; "human-driven";
                  "The chain stops here: this item is human-driven (`eligible=no`). Run it in a session; the chain continues when it closes.")]
              elif ($i.open_blockers // 1) > 0 then .out += [line("wait"; $i.number; "blocked"; "\($i.open_blockers // "unknown") open blocker(s)")]
              elif ($dropped | length) > 0 then .out += [stop($i.number; $i.markers; "blocker-dropped";
                  "The chain stops here: blocker \($dropped | join(", ")) closed without completing. A person decides whether this item still runs; remove the dependency to let it start.")]
              elif .free > 0 then .out += [line("start"; $i.number; "-"; "no open blocker")] | .free -= 1
              else .out += [line("wait"; $i.number; "cap"; "run cap reached (\($cap))")]
              end)
        | .out[]
    ' "$1"
}

case "${1:-}" in
    gather) gather ;;
    decide) decide "$2" ;;
    *) echo "usage: $0 gather | decide <facts.json>" >&2; exit 2 ;;
esac
