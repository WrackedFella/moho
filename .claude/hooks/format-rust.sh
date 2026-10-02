#!/usr/bin/env bash
# PostToolUse hook: rustfmt the edited Rust file so diffs stay formatted.
set -uo pipefail
file="$(jq -r '.tool_input.file_path // empty')"
[[ "$file" == *.rs && -f "$file" ]] || exit 0
rustfmt --edition 2024 "$file" || { echo "rustfmt failed on $file (syntax error?)" >&2; exit 2; }
