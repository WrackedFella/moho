#!/usr/bin/env bash
# Fails when Rust comment lines added since branching from BASE reference ephemeral
# artifacts (work-item IDs, phase/milestone tags, PR or issue numbers). Those belong
# in commit messages and PRs, where they stay accurate.
set -euo pipefail

base="${1:-origin/dev}"
if ! git rev-parse --verify --quiet "$base" >/dev/null; then
    echo "check-comment-refs: base '$base' not found; fetch it (CI needs fetch-depth: 0)" >&2
    exit 1
fi
merge_base="$(git merge-base "$base" HEAD)"

pattern='\b(ENG|SG|FPS)-F[0-9]+(-[0-9]{2})?\b|\bF[0-9]+-[0-9]{2}\b|\b[0-9]+\.[0-9]+[a-z]\b|\b[Pp]hase [0-9A-Z]\b|\b(PR|[Ii]ssue|[Pp]ull) ?#[0-9]+'

hits="$(git diff -U0 "$merge_base" -- '*.rs' \
    | awk '/^\+\+\+ /{file=substr($0,7); next} /^@@/{split($3,a,/[,+]/); line=a[2]; next}
           /^\+/{print file ":" line ": " substr($0,2); line++}' \
    | grep -E '^[^:]+:[0-9]+: *(//|/\*|\*)' \
    | grep -E "$pattern" || true)"

if [[ -n "$hits" ]]; then
    echo "Comments reference work items, phases or PRs; move that context to the commit/PR:" >&2
    echo "$hits" >&2
    exit 1
fi
