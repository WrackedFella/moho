#!/usr/bin/env bash
# Source this file: it replaces `gh` with a wrapper that retries a call refused for
# rate limiting, waiting 20 s, 40 s, then 80 s. Other failures return at once. Every
# call the board scripts make is safe to repeat.
gh() {
  local attempt=0 err rc
  err=$(mktemp)
  while :; do
    rc=0
    command gh "$@" 2>"$err" || rc=$?
    if ((rc == 0)); then
      cat "$err" >&2; rm -f "$err"; return 0
    fi
    if ((attempt < 3)) && grep -qi 'rate limit' "$err"; then
      attempt=$((attempt + 1))
      echo "gh: rate limited, retry $attempt in $((20 * 2 ** (attempt - 1))) s" >&2
      sleep $((20 * 2 ** (attempt - 1)))
      continue
    fi
    cat "$err" >&2; rm -f "$err"; return "$rc"
  done
}
export -f gh
