#!/usr/bin/env bash
# Fails when the workspace crate graph breaks the layering in the table (default
# scripts/layering.txt): every member belongs to exactly one line, the table names only
# members and each line once, engine crates never reach game crates, strategy and fps
# never reach each other, domain crates never reach winit, wgpu or egui, and only crates
# in the table's gpu row depend directly on wgpu, egui-wgpu or naga. Keeps
# engine reuse and platform-free domain code from eroding silently. If
# MOHO_LAYERING_GRAPH names a file, the crate graph is read from it when non-empty and
# written to it otherwise, so repeated runs skip the cargo calls. Each graph line is
# "<crate>: <transitive deps> | <direct deps>".
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
        engine | strategy | fps | domain | gpu) ;;
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

# One line per workspace member: "<crate>: <crates it reaches> | <crates it names directly>".
# Command substitution drops errexit, so each cargo failure returns explicitly; a crate
# with a silently empty dependency list would pass every rule.
collect_graph() {
    local crates crate tree deps direct
    crates="$(cargo tree --workspace --depth 0 -e normal --target all --prefix none --format '{p}' \
        | awk 'NF{print $1}')" || return 1
    for crate in $crates; do
        # Each line is "<depth><package> <version>"; depth 1 is a direct dependency.
        tree="$(cargo tree -p "$crate" -e normal,build,dev --all-features --target all --prefix depth --format '{p}')" \
            || return 1
        deps="$(awk -v self="$crate" '{d = $1; sub(/^[0-9]+/, "", $1)} NF && d !~ /^0/ && $1 != self {print $1}' <<<"$tree" \
            | sort -u | tr '\n' ' ')"
        direct="$(awk -v self="$crate" '/^1[^0-9]/ {sub(/^1/, "", $1); if ($1 != self) print $1}' <<<"$tree" \
            | sort -u | tr '\n' ' ')"
        echo "$crate: $deps| $direct"
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

    reached="${entry#*:}"
    for dep in ${reached%%|*}; do
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

    if ! in_line "$crate" gpu; then
        for dep in ${entry##*|}; do
            case "$dep" in
                wgpu | egui-wgpu | naga) add "gpu-outside-renderer: $crate → $dep" ;;
            esac
        done
    fi
done <<<"$graph"

if [[ -n "$violations" ]]; then
    echo "Layering violations (see _todo/adr/0005-crate-lines-and-dependency-direction.md):" >&2
    printf '%s' "$violations" >&2
    exit 1
fi
echo "layering: OK"
