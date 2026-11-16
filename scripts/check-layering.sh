#!/usr/bin/env bash
# Fails when the workspace crate graph breaks the layering in the table (default
# scripts/layering.txt): every member belongs to exactly one line, the table names only
# members and each line once, engine crates never reach game crates, flagstone and searchlight
# never reach each other, domain crates never reach winit, wgpu or egui, and only crates
# in the table's gpu row reach wgpu, egui-wgpu or naga themselves (dev-dependencies
# count; a path through another workspace member does not), except that crates in the
# gpu-dev row may reach them through dev-dependencies only (ADR-0036). Keeps
# engine reuse and platform-free domain code from eroding silently. If
# MOHO_LAYERING_GRAPH names a file, the crate graph is read from it when non-empty and
# written to it otherwise, so repeated runs skip the cargo calls. MOHO_LAYERING_CARGO
# overrides the cargo command (the self-tests point it at a stub).
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
        engine | flagstone | searchlight | domain | gpu | gpu-dev) ;;
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

# One line per workspace member: "<crate>: <crates it reaches> | <crates it reaches itself>
# | <crates it reaches itself over normal and build edges>". A graph without the third field
# (older fixtures) treats it as equal to the second.
# A crate's own reach is every crate reachable without passing through another workspace
# member, so a third-party wrapper that pulls in wgpu counts against the crate that
# names it, while a path through moho_renderer does not. Two cargo calls per crate: the
# full tree, and a tree with every other member pruned (pruned subtrees are not printed,
# so deduplication cannot hide a path). Dev-dependencies count in both.
# Command substitution drops errexit, so each cargo failure returns explicitly; a crate
# with a silently empty dependency list would pass every rule.
collect_graph() {
    local cargo="${MOHO_LAYERING_CARGO:-cargo}"
    local crates crate other tree own_tree run_tree deps own run prune
    local flags=(--all-features --target all --prefix depth --format '{p}')
    local names_awk='{sub(/^[0-9]+/, "", $1)} NF && $1 != self {print $1}'
    crates="$("$cargo" tree --workspace --depth 0 -e normal --target all --prefix none --format '{p}' \
        | awk 'NF{print $1}')" || return 1
    for crate in $crates; do
        tree="$("$cargo" tree -p "$crate" -e normal,build,dev "${flags[@]}")" || return 1
        # --prune rejects a spec absent from the crate's tree, so prune only the members in it.
        prune=()
        for other in $(awk -v self="$crate" "$names_awk" <<<"$tree" | sort -u); do
            [[ " $crates " == *[[:space:]]"$other"[[:space:]]* && "$other" != "$crate" ]] && prune+=(--prune "$other")
        done
        own_tree="$("$cargo" tree -p "$crate" -e normal,build,dev "${flags[@]}" ${prune[@]+"${prune[@]}"})" || return 1
        run_tree="$("$cargo" tree -p "$crate" -e normal,build "${flags[@]}" ${prune[@]+"${prune[@]}"})" || return 1
        deps="$(awk -v self="$crate" "$names_awk" <<<"$tree" | sort -u | tr '\n' ' ')"
        own="$(awk -v self="$crate" "$names_awk" <<<"$own_tree" | sort -u | tr '\n' ' ')"
        run="$(awk -v self="$crate" "$names_awk" <<<"$run_tree" | sort -u | tr '\n' ' ')"
        echo "$crate: $deps| $own| $run"
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
    in_engine="" in_flagstone="" in_searchlight="" in_domain="" count=0
    in_line "$crate" engine && in_engine=1 && count=$((count + 1))
    in_line "$crate" flagstone && in_flagstone=1 && count=$((count + 1))
    in_line "$crate" searchlight && in_searchlight=1 && count=$((count + 1))
    in_line "$crate" domain && in_domain=1
    [[ $count -eq 0 ]] && add "unassigned: $crate"
    [[ $count -gt 1 ]] && add "duplicate: $crate"

    reached="${entry#*:}"
    for dep in ${reached%%|*}; do
        if [[ -n "$in_engine" ]] && { in_line "$dep" flagstone || in_line "$dep" searchlight; }; then
            add "engine-reaches-game: $crate → $dep"
        fi
        if { [[ -n "$in_flagstone" ]] && in_line "$dep" searchlight; } \
            || { [[ -n "$in_searchlight" ]] && in_line "$dep" flagstone; }; then
            add "cross-game-line: $crate → $dep"
        fi
        if [[ -n "$in_domain" ]]; then
            case "$dep" in
                winit | wgpu | egui) add "domain-reaches-platform: $crate → $dep" ;;
            esac
        fi
    done

    reach="${reached#*|}"
    own="${reach%%|*}" run="$own"
    [[ "$reach" == *"|"* ]] && run="${reach#*|}"
    if in_line "$crate" gpu; then
        :
    elif in_line "$crate" gpu-dev; then
        for dep in $run; do
            case "$dep" in
                wgpu | egui-wgpu | naga) add "gpu-at-runtime: $crate → $dep" ;;
            esac
        done
    else
        for dep in $own; do
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
