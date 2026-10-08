#!/usr/bin/env bash
# One-time repository setup: labels and branch protection for dev and main.
# Idempotent; run by the repo owner (needs admin), never by agents.
#   scripts/github-setup.sh labels        create labels
#   scripts/github-setup.sh protect       apply branch protection
set -euo pipefail

repo="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"

labels() {
    gh label create agent-ready --force --color 0E8A16 --description "Starts an orchestrator run on this issue; apply only to Ready, Agent-eligible items"
    gh label create feature --force --color 5319E7 --description "Parent issue for an approved feature"
    gh label create line:engine --force --color 1D76DB --description "Engine crates"
    gh label create line:strategy --force --color D93F0B --description "Strategy/base-builder game"
    gh label create line:fps --force --color FBCA04 --description "FPS prototype"
}

# Solo-owner settings: PR required but no approval count (can't self-approve);
# status checks must pass and be up to date; no force-push or deletion.
protect() {
    local branch="$1"; shift
    local contexts
    contexts="$(printf '%s\n' "$@" | jq -R . | jq -sc .)"
    gh api -X PUT "repos/$repo/branches/$branch/protection" --input - <<JSON
{
  "required_status_checks": { "strict": true, "contexts": $contexts },
  "enforce_admins": false,
  "required_pull_request_reviews": { "required_approving_review_count": 0 },
  "restrictions": null,
  "allow_force_pushes": false,
  "allow_deletions": false
}
JSON
}

case "${1:-}" in
    labels) labels ;;
    protect)
        protect dev "Gate (ubuntu-latest)" "Deny"
        protect main "Gate (ubuntu-latest)" "Gate (macos-latest)" "Gate (windows-latest)" "Deny"
        ;;
    *) echo "usage: $0 labels|protect" >&2; exit 2 ;;
esac
