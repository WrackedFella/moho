#!/usr/bin/env bash
# Stop hook: block finishing while uncommitted Rust/Cargo changes fail the gate.
set -uo pipefail

input="$(cat)"
# A second stop attempt after a block is let through so a stuck gate can't loop.
[[ "$(jq -r '.stop_hook_active // false' <<<"$input")" == "true" ]] && exit 0

cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0
if git diff --quiet HEAD -- '*.rs' '*Cargo.toml' 'Cargo.lock' 2>/dev/null \
    && [[ -z "$(git ls-files --others --exclude-standard -- '*.rs')" ]]; then
    exit 0
fi

if ! output="$(just check 2>&1)"; then
    echo "just check failed on uncommitted changes; fix before finishing:" >&2
    tail -n 40 <<<"$output" >&2
    exit 2
fi
