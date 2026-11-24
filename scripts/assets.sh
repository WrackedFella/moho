#!/usr/bin/env bash
# Generated assets: `write` reruns every generator in place; `check` regenerates into a
# scratch directory and fails, naming each file, when a committed file differs. Generators
# are standard-library Python and take an output directory. MOHO_ASSETS_ROOT replaces the
# repository root that committed files are read from (self-test only).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# "<generator> <directory its outputs are committed in>"
generators=(
  "searchlight/assets/tools/make_arena.py searchlight/assets"
  "flagstone/assets/tools/make_pickup.py flagstone/assets"
  "flagstone/assets/tools/make_dig.py flagstone/assets"
)

mode="${1:-}"
case "$mode" in
  write | check) ;;
  *) echo "usage: scripts/assets.sh write|check" >&2; exit 64 ;;
esac

root="${MOHO_ASSETS_ROOT:-.}"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
stale=()
for entry in "${generators[@]}"; do
  read -r generator dir <<<"$entry"
  if [[ "$mode" == write ]]; then
    python3 -I "$generator" "$dir"
    continue
  fi
  out="$scratch/$(basename "$generator" .py)"
  mkdir -p "$out"
  python3 -I "$generator" "$out"
  for made in "$out"/*; do
    committed="$root/$dir/$(basename "$made")"
    cmp -s "$made" "$committed" || stale+=("$dir/$(basename "$made")")
  done
done

if ((${#stale[@]})); then
  echo "assets: stale (run \`just assets\` and commit): ${stale[*]}" >&2
  exit 1
fi
echo "assets: $mode OK"
