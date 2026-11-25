#!/usr/bin/env bash
# Fails when an engine sim file calls a platform transcendental. The files are the
# paths listed in the file named by the first argument (default scripts/sim-math.txt),
# one per line, relative to the repository root; '#' starts a comment. A listed file
# that does not exist fails the check and is named. In each file the check flags method
# calls (.sin(), .sin_cos(), .powf(), ...) and f32:: / f64:: path calls of the names
# below, and prints file:line with the replacement, moho_core::sim_math. sqrt, abs,
# to_radians, powi and glam vector ops are not flagged. Test code is exempt: a
# #[cfg(test)] followed by an inline `mod name {` ends the scan of the file, and any
# other #[cfg(test)] exempts only the item on the next line. The list is the boundary:
# files absent from it (camera, rendering, UI) are never scanned. A grep-level check:
# calls through a trait or an alias are not seen. Plain POSIX awk, so it runs the same
# on Linux, macOS and Windows git-bash.
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
            call = "(\\.|(^|[^A-Za-z0-9_])(f32|f64)::)(" names ")[[:space:]]*\\("
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
            if (line ~ call) printf "%s:%d: platform transcendental; use moho_core::sim_math\n", file, NR
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
