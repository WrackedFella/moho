#!/usr/bin/env bash
# Starts the GitNexus MCP server for this repo (see .mcp.json), installing GitNexus and
# building its index first when they are missing. The server starts before anything else
# in a session runs and is never restarted, so a fresh cloud clone must be able to
# bring itself up here rather than wait for a setup step.
#
#   gitnexus-mcp.sh           install if needed, index in the background, then serve
#   gitnexus-mcp.sh --setup   install and index to completion, then exit (cloud-tools.sh)
#
# stdout carries the MCP protocol, so every other command writes to stderr or a log.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
bin=node_modules/.bin/gitnexus
log="${TMPDIR:-/tmp}/gitnexus-index.log"

# Serializes this script against itself and cloud-tools.sh; each step re-checks its
# own precondition after taking the lock, so only the first caller does the work.
locked() {
  if command -v flock >/dev/null; then
    ( flock 9; "$@" ) 9>"${TMPDIR:-/tmp}/gitnexus-setup.lock"
  else
    "$@"
  fi
}

install_deps() {
  [ -x "$bin" ] && return 0
  # ONNXRUNTIME_NODE_INSTALL=skip: onnxruntime-node's postinstall downloads from
  # nuget.org, which the cloud sandbox proxy resets; it only serves embeddings (off).
  ONNXRUNTIME_NODE_INSTALL=skip npm ci --no-audit --no-fund >&2
}

# --skip-agents-md keeps analyze from rewriting tracked files such as CLAUDE.md. Full-text
# search stays off in the sandbox (its extension downloads from ladybugdb.com); impact,
# context and detect_changes do not need it.
build_index() {
  [ -d .gitnexus ] || "$bin" analyze --skip-agents-md
}

if [ "${1:-}" = --setup ]; then
  # Optional tooling: a failure must not fail the run.
  locked install_deps >&2 && locked build_index >&2 \
    || echo "GitNexus setup failed; GitNexus unavailable this session" >&2
  exit 0
fi

locked install_deps
# The server reads the index registry on every call, so the index can finish after the
# handshake; stdin is detached so the indexer cannot consume protocol input.
if [ ! -d .gitnexus ]; then
  ( locked build_index </dev/null >"$log" 2>&1 & )
fi
exec "$bin" mcp
