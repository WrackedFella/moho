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
[[ -n "$out" ]] || { echo "package: no output folder given" >&2; exit 1; }

# The physical path of an existing folder.
physical() { (cd "$1" && pwd -P); }

# The output folder's physical path, whether or not it exists yet.
if [[ -d "$out" ]]; then
    resolved=$(physical "$out")
elif [[ -d "$(dirname "$out")" ]]; then
    resolved=$(physical "$(dirname "$out")")/$(basename "$out")
else
    resolved=""
fi

# Emptying the output must never take the working directory or the executable with it.
if [[ -n "$resolved" ]]; then
    for kept in "$(pwd -P)" "$(physical "$(dirname "$exe")")"; do
        if [[ "$resolved" == / || "$kept" == "$resolved" || "$kept" == "$resolved"/* ]]; then
            echo "package: refusing output folder '$out': it is or holds $kept" >&2
            exit 1
        fi
    done
fi

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
