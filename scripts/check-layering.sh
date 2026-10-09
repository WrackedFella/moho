#!/usr/bin/env bash
# Fails when the workspace crate graph breaks the layering in the table (default
# scripts/layering.txt): every member belongs to exactly one line, the table names only
# members and each line once, engine crates never reach game crates, strategy and fps
# never reach each other, and domain crates never reach winit, wgpu or egui. Keeps
# engine reuse and platform-free domain code from eroding silently. If
# MOHO_LAYERING_GRAPH names a file, the crate graph is read from it when non-empty and
# written to it otherwise, so repeated runs skip the cargo calls.
set -euo pipefail

table="${1:-scripts/layering.txt}"
if [[ ! -f "$table" ]]; then
    echo "check-layering: table '$table' not found" >&2
    exit 1
fi

violations=""
add() { violations+="$1"$'\n'; }

rows=" " names="" seen=" "
while IFS= read -r raw || [[ -n "$raw" ]]; do
    row="${raw%$'\r'}"
    row="${row%%#*}"
    [[ "$row" =~ ^[[:space:]]*$ ]] && continue
    name="${row%%:*}"
    name="${name//[[:space:]]/}"
    case "$name" in
        engine | strategy | fps | domain) ;;
        *) echo "check-layering: unknown line '$name' in $table" >&2; exit 1 ;;
    esac
    [[ "$seen" == *" $name "* ]] && add "repeated row: $name"
    seen+="$name "
    for c in ${row#*:}; do
        rows+="$name:$c "
        names+="$c"$'\n'
    done
done <"$table"

in_line() { [[ "$rows" == *" $2:$1 "* ]]; }

# One line per workspace member: "<crate>: <crate it reaches> ...".
# Command substitution drops errexit, so each cargo failure returns explicitly; a crate
# with a silently empty dependency list would pass every rule.
collect_graph() {
    local crates crate deps
    crates="$(cargo tree --workspace --depth 0 -e normal --target all --prefix none --format '{p}' \
        | awk 'NF{print $1}')" || return 1
    for crate in $crates; do
        deps="$(cargo tree -p "$crate" -e normal,build,dev --all-features --target all --prefix none --format '{p}' \
            | awk -v self="$crate" 'NF && $1 != self {print $1}' | sort -u | tr '\n' ' ')" || return 1
        echo "$crate: $deps"
    done
}

cache="${MOHO_LAYERING_GRAPH:-}"
if [[ -n "$cache" && -s "$cache" ]]; then
    graph="$(cat "$cache")"
else
    graph="$(collect_graph)"
    [[ -n "$cache" ]] && printf '%s\n' "$graph" >"$cache"
fi

members=""
while IFS= read -r entry; do
    members+=" ${entry%%:*} "
done <<<"$graph"

for named in $(printf '%s' "$names" | sort -u); do
    [[ "$members" == *" $named "* ]] || add "unknown: $named"
done

while IFS= read -r entry; do
    crate="${entry%%:*}"
    in_engine="" in_strategy="" in_fps="" in_domain="" count=0
    in_line "$crate" engine && in_engine=1 && count=$((count + 1))
    in_line "$crate" strategy && in_strategy=1 && count=$((count + 1))
    in_line "$crate" fps && in_fps=1 && count=$((count + 1))
    in_line "$crate" domain && in_domain=1
    [[ $count -eq 0 ]] && add "unassigned: $crate"
    [[ $count -gt 1 ]] && add "duplicate: $crate"

    for dep in ${entry#*:}; do
        if [[ -n "$in_engine" ]] && { in_line "$dep" strategy || in_line "$dep" fps; }; then
            add "engine-reaches-game: $crate → $dep"
        fi
        if { [[ -n "$in_strategy" ]] && in_line "$dep" fps; } \
            || { [[ -n "$in_fps" ]] && in_line "$dep" strategy; }; then
            add "cross-game-line: $crate → $dep"
        fi
        if [[ -n "$in_domain" ]]; then
            case "$dep" in
                winit | wgpu | egui) add "domain-reaches-platform: $crate → $dep" ;;
            esac
        fi
    done
done <<<"$graph"

if [[ -n "$violations" ]]; then
    echo "Layering violations (see _todo/adr/0005-crate-lines-and-dependency-direction.md):" >&2
    printf '%s' "$violations" >&2
    exit 1
fi
echo "layering: OK"
