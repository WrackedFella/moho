#!/usr/bin/env bash
# Fails when the workspace crate graph breaks the layering in the table (default
# scripts/layering.txt): every member belongs to exactly one line, engine crates never
# reach game crates, strategy and fps never reach each other, and domain crates never
# reach winit, wgpu or egui. Keeps engine reuse and platform-free domain code from
# eroding silently.
set -euo pipefail

table="${1:-scripts/layering.txt}"
if [[ ! -f "$table" ]]; then
    echo "check-layering: table '$table' not found" >&2
    exit 1
fi

violations=""
add() { violations+="$1"$'\n'; }

engine="" strategy="" fps="" domain="" seen=" "
while IFS= read -r raw || [[ -n "$raw" ]]; do
    row="${raw%$'\r'}"
    row="${row%%#*}"
    [[ "$row" =~ ^[[:space:]]*$ ]] && continue
    name="${row%%:*}"
    name="${name//[[:space:]]/}"
    crates=" ${row#*:} "
    case "$name" in
        engine | strategy | fps | domain) ;;
        *) echo "check-layering: unknown line '$name' in $table" >&2; exit 1 ;;
    esac
    [[ "$seen" == *" $name "* ]] && add "repeated row: $name"
    seen+="$name "
    case "$name" in
        engine) engine+="$crates" ;;
        strategy) strategy+="$crates" ;;
        fps) fps+="$crates" ;;
        domain) domain+="$crates" ;;
    esac
done <"$table"

in_set() { [[ "$2" == *" $1 "* ]]; }

members="$(cargo tree --workspace --depth 0 -e normal --target all --prefix none --format '{p}' | awk 'NF{print $1}')"

known=" $(echo $members) "
reported=" "
for named in $engine $strategy $fps $domain; do
    if ! in_set "$named" "$known" && ! in_set "$named" "$reported"; then
        add "unknown: $named"
        reported+="$named "
    fi
done

for crate in $members; do
    lines=0
    in_set "$crate" "$engine" && lines=$((lines + 1))
    in_set "$crate" "$strategy" && lines=$((lines + 1))
    in_set "$crate" "$fps" && lines=$((lines + 1))
    [[ $lines -eq 0 ]] && add "unassigned: $crate"
    [[ $lines -gt 1 ]] && add "duplicate: $crate"

    deps="$(cargo tree -p "$crate" -e normal,build --all-features --target all --prefix none --format '{p}' \
        | awk -v self="$crate" 'NF && $1 != self {print $1}' | sort -u)"
    for dep in $deps; do
        if in_set "$crate" "$engine" && { in_set "$dep" "$strategy" || in_set "$dep" "$fps"; }; then
            add "engine-reaches-game: $crate → $dep"
        fi
        if { in_set "$crate" "$strategy" && in_set "$dep" "$fps"; } \
            || { in_set "$crate" "$fps" && in_set "$dep" "$strategy"; }; then
            add "cross-game-line: $crate → $dep"
        fi
        if in_set "$crate" "$domain"; then
            case "$dep" in
                winit | wgpu | egui) add "domain-reaches-platform: $crate → $dep" ;;
            esac
        fi
    done
done

if [[ -n "$violations" ]]; then
    echo "Layering violations (see _todo/adr/0005-crate-lines-and-dependency-direction.md):" >&2
    printf '%s' "$violations" >&2
    exit 1
fi
echo "layering: OK"
