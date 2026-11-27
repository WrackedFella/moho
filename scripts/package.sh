#!/usr/bin/env bash
# Lays out a game's package: scripts/package.sh <game> <exe> <out>.
# Empties <out>, copies the executable, then the git-tracked files of <game>/assets and
# <game>/content (those that exist) except tools/ and *.md, keeping their relative layout.
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: $0 <game> <exe> <out>" >&2
    exit 2
fi
game=$1
exe=$2
out=$3

[[ -f "$exe" ]] || { echo "package: executable $exe not found" >&2; exit 1; }
[[ -n "$out" && "$out" != "/" ]] || { echo "package: refusing output folder '$out'" >&2; exit 1; }

rm -rf "$out"
mkdir -p "$out"
cp "$exe" "$out/"

for folder in assets content; do
    [[ -d "$game/$folder" ]] || continue
    while IFS= read -r file; do
        case "$file" in
            */tools/* | *.md) continue ;;
        esac
        rel=${file#"$game"/}
        mkdir -p "$out/$(dirname "$rel")"
        cp "$file" "$out/$rel"
    done < <(git ls-files "$game/$folder")
done
