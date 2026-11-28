#!/usr/bin/env bash
# Prints the domain-logic globs from CLAUDE.md, one per line: the indented bullets under
# its "Domain-logic paths" item. `just mutants` scopes mutation testing to them.
# Fails when a glob matches no tracked file, so a rename can't silently drop code from scope.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

globs=$(awk '
    /^- \*\*Domain-logic paths\*\*/ { inside = 1; next }
    /^- / { inside = 0 }
    inside && match($0, /^  - `[^`]+`/) { print substr($0, 6, RLENGTH - 6) }
' CLAUDE.md)

if [[ -z $globs ]]; then
    echo "domain-paths: no globs under 'Domain-logic paths' in CLAUDE.md" >&2
    exit 1
fi

while IFS= read -r glob; do
    if [[ -z $(git ls-files -- "$glob") ]]; then
        echo "domain-paths: '$glob' matches no tracked file" >&2
        exit 1
    fi
done <<< "$globs"

echo "$globs"
