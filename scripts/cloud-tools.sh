#!/usr/bin/env bash
# Installs what `just check`, `just mutants` and `just deny` need in a fresh Linux cloud
# container: native deps, the pinned toolchain, and the gate tools. Idempotent, and each
# step skips itself when its work is present, so a warm container finishes in seconds.
# Run it from any thread that builds or tests; threads that only edit text do not need it.
# Gate tools come as prebuilt binaries only: compiling them from source takes ~10 minutes.
set -euo pipefail

# Installs apt packages with sudo, so it must not run on a developer machine.
if [[ "${CLAUDE_CODE_REMOTE:-}" != true && "${1:-}" != --force ]]; then
  echo "cloud-tools: not a cloud thread (CLAUDE_CODE_REMOTE is not true); pass --force to run anyway" >&2
  exit 1
fi

cd "$(dirname "${BASH_SOURCE[0]}")/.."
export PATH="$HOME/.cargo/bin:$PATH"
log="$(mktemp -d)"
pids=(); names=()

step() { # step <name> <fn>: run in the background and record its duration
  local name="$1"; shift
  ( s=$SECONDS; rc=0; "$@" >"$log/$name.log" 2>&1 || rc=$?
    echo "cloud-tools[$name] rc=$rc $((SECONDS - s))s" >>"$log/timing"; exit "$rc" ) &
  pids+=("$!"); names+=("$name")
}

apt_deps() {
  local pkgs=(pkg-config libudev-dev libasound2-dev libssl-dev)
  dpkg -s "${pkgs[@]}" >/dev/null 2>&1 && return 0
  sudo apt-get update -qq
  sudo apt-get install -y --no-install-recommends "${pkgs[@]}"
}

rust_toolchain() {
  command -v rustup >/dev/null \
    || curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
  # rust-toolchain.toml is the single source for the version and components.
  rustup toolchain install "$(sed -nE 's/^channel *= *"([^"]+)".*/\1/p' rust-toolchain.toml)" \
    --profile minimal -c rustfmt -c clippy
}

gate_tools() {
  if ! command -v cargo-binstall >/dev/null; then
    mkdir -p "$HOME/.cargo/bin"
    curl -fsSL https://github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-x86_64-unknown-linux-musl.tgz \
      | tar -xz -C "$HOME/.cargo/bin"
  fi
  local missing=() t
  for t in just cargo-nextest cargo-mutants cargo-deny; do
    command -v "$t" >/dev/null || missing+=("$t")
  done
  [ ${#missing[@]} -eq 0 ] || cargo binstall -y --locked --disable-strategies compile "${missing[@]}"
}

node_deps() {
  # GitNexus is optional: a failed install must not fail the run.
  [ -f package-lock.json ] && [ ! -d node_modules ] || return 0
  npm ci --no-audit --no-fund || echo "npm ci failed; GitNexus unavailable this session"
}

# Runs first and alone: every cargo call below would otherwise start its own
# toolchain download from rust-toolchain.toml and race this one over rustup's files.
rust_toolchain >"$log/rust.log" 2>&1 || { echo "cloud-tools step 'rust' FAILED:"; cat "$log/rust.log"; exit 1; }

step apt apt_deps
step tools gate_tools
step node node_deps

fail=0
for i in "${!pids[@]}"; do
  wait "${pids[$i]}" || { echo "cloud-tools step '${names[$i]}' FAILED:"; cat "$log/${names[$i]}.log"; fail=1; }
done
cat "$log/timing"
exit "$fail"
