#!/usr/bin/env bash
# Decides whether an agent PR may merge without a person, for pr-review.yml's merge step.
#
#   merge-eligibility.sh gather <pr>     prints the PR's facts as JSON (needs gh, GH_REPO,
#                                        REVIEWER, PR_AUTOMERGE)
#   merge-eligibility.sh decide <facts>  prints `eligible <class>` and exits 0, or one
#                                        `ineligible: <reason>` line per failed rule and
#                                        exits 1
#
# `decide` is pure, so the rules are tested against fixtures (`just merge-eligibility-test`).
# Any missing fact fails closed. The required checks are not judged here: the ruleset on
# `dev` enforces them on the merge itself.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Paths whose change always needs a person: what the gate checks, how agents run, and the
# written rules a review judges against.
DENY='^(\.github|\.claude|\.cargo|\.config|\.husky|\.gitnexus|scripts|_todo/adr|wiki/process)/|^[^/]*\.toml$|^(justfile|Cargo\.lock|\.mcp\.json)$|(^|/)CLAUDE\.md$|^_todo/(_STANDARDS|WORKFLOW)\.md$'

gather() {
    local pr=$1 pull issue_no issue='null' comments base_ci domain advisory
    : "${GH_REPO:?}" "${REVIEWER:?}"
    pull=$(gh api "repos/$GH_REPO/pulls/$pr")
    issue_no=$(jq -r '[.body // "" | scan("(?i)\\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?) #([0-9]+)") | .[0]] | unique
        | if length == 1 then .[0] else "" end' <<<"$pull")
    comments=$(
        gh api "repos/$GH_REPO/issues/$pr/comments" --paginate --jq '.[] | {login: .user.login, assoc: .author_association, created_at}'
        gh api "repos/$GH_REPO/pulls/$pr/comments" --paginate --jq '.[] | {login: .user.login, assoc: .author_association, created_at}'
        if [[ -n $issue_no ]]; then
            gh api "repos/$GH_REPO/issues/$issue_no/comments" --paginate --jq '.[] | {login: .user.login, assoc: .author_association, created_at}'
        fi
    )
    if [[ -n $issue_no ]]; then
        issue=$(gh api "repos/$GH_REPO/issues/$issue_no" --jq '{number, state, labels: [.labels[].name]}')
    fi
    base_ci=$(gh api "repos/$GH_REPO/actions/workflows/ci.yml/runs?branch=$(jq -r .base.ref <<<"$pull")&event=push&status=completed&per_page=1" \
        --jq '.workflow_runs[0] // {} | {sha: .head_sha, conclusion}')
    domain=$(scripts/domain-paths.sh | jq -R . | jq -s .)
    # The `Advisory reviewers` logins: the backticked items on that CLAUDE.md line.
    advisory=$(grep -m1 'Advisory reviewers:' CLAUDE.md | grep -o '`[^`]*`' | tr -d '`' | jq -R . | jq -s .)
    jq -n \
        --argjson pull "$pull" \
        --argjson files "$(gh api "repos/$GH_REPO/pulls/$pr/files" --paginate --jq '.[].filename' | jq -R . | jq -s .)" \
        --argjson reviews "$(gh api "repos/$GH_REPO/pulls/$pr/reviews" --paginate --jq '.[] | {login: .user.login, assoc: .author_association, state, commit_id, submitted_at, head: (.body // "" | split("\n") | .[0:4] | join("\n"))}' | jq -s .)" \
        --argjson comments "$(jq -s . <<<"$comments")" \
        --argjson issue "$issue" \
        --argjson base_ci "$base_ci" \
        --argjson domain "$domain" \
        --argjson advisory "$advisory" \
        --arg reviewer "$REVIEWER" \
        --arg scope "${PR_AUTOMERGE:-off}" \
        --arg repo "$GH_REPO" \
        '{
            scope: $scope, repo: $repo, reviewer: $reviewer, domain: $domain, advisory: $advisory,
            pr: {
                number: $pull.number, state: $pull.state, draft: $pull.draft,
                author: $pull.user.login, head_repo: ($pull.head.repo.full_name // null),
                base: $pull.base.ref, head_sha: $pull.head.sha, labels: [$pull.labels[].name]
            },
            files: $files, reviews: $reviews, comments: $comments, issue: $issue, base_ci: $base_ci
        }'
}

decide() {
    local out
    out=$(jq -r --arg deny "$DENY" '
        # A CLAUDE.md domain glob as an anchored regex: `**` spans directories, `*` does not.
        def glob_re: "^" + (gsub("\\."; "\\.") | gsub("\\*\\*"; "\u0000") | gsub("\\*"; "[^/]*") | gsub("\u0000"; ".*")) + "$";
        def field($name): (capture("(?m)^.*\\b" + $name + ": *(?<v>[a-z0-9]+)") | .v) // "";
        . as $f
        | ["claude[bot]", "github-actions[bot]", $f.reviewer] as $bots
        | ($f.domain | map(glob_re)) as $domain
        | (if any($f.files[]; . as $p | any($domain[]; . as $re | $p | test($re))) then "domain" else "glue" end) as $class
        | ([$f.reviews[] | select(.login == $f.reviewer and (.head | startswith("Verdict:")))] | sort_by(.submitted_at) | last) as $review
        | [
            (if $f.scope == "glue" or $f.scope == "all" then empty else "merging is off (PR_AUTOMERGE=\($f.scope))" end),
            (if $f.scope == "glue" and $class == "domain" then "domain change; PR_AUTOMERGE=glue merges glue only" else empty end),
            (if $f.pr.state == "open" and ($f.pr.draft | not) then empty else "PR is \($f.pr.state)\(if $f.pr.draft then ", draft" else "" end)" end),
            (if $f.pr.author == "claude[bot]" or $f.pr.author == "github-actions[bot]" then empty else "opened by \($f.pr.author), not an agent run" end),
            (if $f.pr.head_repo == $f.repo then empty else "head is in \($f.pr.head_repo // "a deleted repo"), not \($f.repo)" end),
            (if $f.pr.base == "dev" then empty else "base is \($f.pr.base), not dev" end),
            (if ($f.files | length) > 0 then empty else "no changed files listed" end),
            ($f.files[] | select(test($deny)) | "changes \(.), which needs a person"),
            (if $review == null then "no review from \($f.reviewer)"
             elif $review.commit_id != $f.pr.head_sha then "latest review is on \($review.commit_id), head is \($f.pr.head_sha)"
             else
                (if ($review.head | field("Verdict")) == "approve" then empty else "verdict is \($review.head | field("Verdict"))" end),
                (if ($review.head | field("Sign-off")) == "agent" then empty else "sign-off is \($review.head | field("Sign-off"))" end)
             end),
            ($f.reviews[] | select(.state == "CHANGES_REQUESTED" and (.login as $l | $bots | index($l) | not)) | "\(.login) requested changes"),
            # An advisory reviewer comment predating the verdict was input to that review.
            ($f.comments + $f.reviews | .[]
                | select((.login as $l | $bots | index($l)) or (.assoc | IN("OWNER", "MEMBER", "COLLABORATOR")) | not)
                | if (.login | IN(($f.advisory // [])[])) then
                    (if $review != null and (.created_at // .submitted_at) != null and (.created_at // .submitted_at) <= $review.submitted_at then empty
                     else "advisory comment from \(.login) after the review" end)
                  else "comment from \(.login) (\(.assoc)), not a repository member" end),
            (if $f.issue == null then "no single closing issue reference in the PR body"
             elif $f.issue.state != "open" then "issue #\($f.issue.number) is \($f.issue.state)"
             else empty end),
            ($f.pr.labels + ($f.issue.labels // []) | .[] | select(IN("needs-human", "agent-paused")) | "labelled \(.)"),
            (if $f.base_ci.conclusion == "success" then empty else "last CI on \($f.pr.base) is \($f.base_ci.conclusion // "missing")" end)
          ] | unique as $why
        | if ($why | length) == 0 then "eligible \($class)" else ($why[] | "ineligible: \(.)") end
    ' "$1")
    printf '%s\n' "$out"
    [[ $out == eligible* ]]
}

case "${1:-}" in
    gather) gather "$2" ;;
    decide) decide "$2" ;;
    *) echo "usage: $0 gather <pr> | decide <facts.json>" >&2; exit 2 ;;
esac
