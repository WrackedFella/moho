#!/usr/bin/env bash
# Fails when an engine sim file calls a platform transcendental, which can differ per OS
# and libm; sim code uses moho_core::sim_math instead. The files to scan are listed in the
# file named by the first argument (default scripts/sim-math.txt): one repo-relative path
# per line, '#' starts a comment, and a listed file that does not exist fails the check.
# Flagged, as file:line with the replacement: method calls (.name(...)) and f32::, f64::,
# <f32>:: and <f64>:: paths (called or passed as a function value) of the names in
# `names=` below. Test code is exempt: #[cfg(test)] followed by an inline `mod name {`
# ends the scan of the file, and any other #[cfg(test)] exempts only the next line. The
# list is the boundary: files outside it (camera, rendering, UI) are never scanned. It is
# a textual check, so calls through a trait or an alias go unseen. Plain POSIX awk, so it
# runs the same on Linux, macOS and Windows git-bash.
set -euo pipefail

list="${1:-scripts/sim-math.txt}"
if [[ ! -f "$list" ]]; then
    echo "check-sim-math: list '$list' not found" >&2
    exit 1
fi

names='sin|cos|tan|asin|acos|atan|atan2|sin_cos|sinh|cosh|tanh|exp|exp2|ln|log|log2|log10|powf|cbrt|hypot'
failed=0

while IFS= read -r raw || [[ -n "$raw" ]]; do
    path="${raw%$'\r'}"
    path="${path%%#*}"
    path="${path#"${path%%[![:space:]]*}"}"
    path="${path%"${path##*[![:space:]]}"}"
    [[ -z "$path" ]] && continue
    if [[ ! -f "$path" ]]; then
        echo "check-sim-math: listed file '$path' not found" >&2
        failed=1
        continue
    fi
    hits=$(awk -v file="$path" -v names="$names" '
        BEGIN {
            method = "\\.(" names ")[[:space:]]*\\("
            path = "((^|[^A-Za-z0-9_])(f32|f64)|<(f32|f64)>)::(" names ")([^A-Za-z0-9_]|$)"
            gate = "^[[:space:]]*#\\[cfg\\(test\\)\\][[:space:]]*$"
            inline_mod = "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?mod[[:space:]]+[A-Za-z0-9_]+[[:space:]]*\\{"
            pending = 0
        }
        {
            line = $0
            sub(/\r$/, "", line)
            if (pending) {
                if (line ~ /^[[:space:]]*#\[/) next
                pending = 0
                if (line ~ inline_mod) exit
                next
            }
            if (line ~ gate) { pending = 1; next }
            sub(/\/\/.*/, "", line)
            if (line ~ method || line ~ path) printf "%s:%d: platform transcendental; use moho_core::sim_math\n", file, NR
        }
    ' "$path")
    if [[ -n "$hits" ]]; then
        echo "$hits" >&2
        failed=1
    fi
done <"$list"

if [[ "$failed" -ne 0 ]]; then
    exit 1
fi
echo "sim-math: OK"
