#!/usr/bin/env bash
# Merge the weekly mutation run's shard outputs.
#
#   mutants-merge.sh <shards-dir> <shard-count> <out-dir>
#
# <shards-dir> holds one `mutants-shard-<k>` folder per shard (a cargo-mutants
# `mutants.out`). Writes merged caught/missed/timeout/unviable lists, a copy of every
# shard and `summary.md` to <out-dir>, and prints the totals on one line. Exits 1 when
# any shard is missing or incomplete, after writing everything it could.
set -euo pipefail
shopt -s nullglob

shards=$1 count=$2 out=$3
mkdir -p "$out"

complete=() missing=() incomplete=()
for ((k = 0; k < count; k++)); do
    f=$shards/mutants-shard-$k/outcomes.json
    if [[ ! -f $f ]]; then
        missing+=("$k")
    # cargo-mutants rewrites outcomes.json after each mutant and sets end_time only once
    # it finishes; a failed baseline finishes with no mutants tested.
    elif jq -e '.end_time != null and .total_mutants > 0' "$f" > /dev/null 2>&1; then
        complete+=("$f")
    else
        incomplete+=("$k")
    fi
done

for list in caught missed timeout unviable; do
    cat "$shards"/*/"$list".txt /dev/null | sort > "$out/$list.txt"
done
cp -r "$shards" "$out/shards"

totals='no complete shard'
if ((${#complete[@]})); then
    totals=$(jq -rs '"caught \(map(.caught) | add)  missed \(map(.missed) | add)" +
        "  unviable \(map(.unviable) | add)  timeout \(map(.timeout) | add)"' "${complete[@]}")
fi

{
    echo '## Weekly mutation run'
    echo
    echo "Totals over ${#complete[@]} of $count shards: $totals"
    if ((${#missing[@]})); then
        echo
        echo "**Shards that reported nothing:** ${missing[*]}"
    fi
    if ((${#incomplete[@]})); then
        echo
        echo "**Shards that did not finish (not counted; their lists are merged):** ${incomplete[*]}"
    fi
} > "$out/summary.md"

echo "$totals; complete ${#complete[@]}/$count; missing: ${missing[*]:-none}; incomplete: ${incomplete[*]:-none}"
((${#missing[@]} + ${#incomplete[@]} == 0))
