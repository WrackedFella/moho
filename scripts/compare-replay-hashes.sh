#!/usr/bin/env bash
# Compare the replay state hashes the three CI platforms wrote.
#
#   compare-replay-hashes.sh <hashes-dir>
#   compare-replay-hashes.sh --self-test
#
# <hashes-dir> holds one `<platform>/hashes.txt` per platform (ubuntu-latest,
# macos-latest, windows-latest), each line `<tick> <hash:016x>`. Prints a Markdown
# summary on stdout:
#
#   ## Replay state hashes
#
#   | Platform | Final tick | Final-tick hash |      one row per platform with hashes,
#   |---|---|---|                                    sorted by platform name
#   | <platform> | <tick> | <hash> |
#
#   agree: yes|no              all platforms with hashes share every tick's hash
#   missing: <platforms>|none  platforms with no hashes.txt, comma-space separated
#
#   first disagreement: tick <n>              only when agree is no
#   - <hash>: <platform>, <platform>          one line per distinct hash at that tick,
#                                             ordered by first platform name
#
# Disagreement and a missing platform are reported, never a failure: exits 0. Exits 2
# on a usage error. `--self-test` runs every case under scripts/fixtures/replay-hashes
# and diffs the output against the case's expected.md.
set -euo pipefail

compare() {
    echo "compare-replay-hashes: not implemented" >&2
    return 1
}

self_test() {
    local fixtures failed=0 case_dir name actual
    fixtures="$(dirname "${BASH_SOURCE[0]}")/fixtures/replay-hashes"
    for name in agree disagree missing; do
        case_dir=$fixtures/$name
        if ! actual=$(compare "$case_dir"); then
            echo "FAIL $name: compare exited non-zero" >&2
            failed=1
        elif ! diff <(printf '%s\n' "$actual") "$case_dir/expected.md"; then
            echo "FAIL $name: output differs from expected.md" >&2
            failed=1
        else
            echo "ok $name"
        fi
    done
    if "${BASH_SOURCE[0]}" > /dev/null 2>&1; then
        echo "FAIL usage: no arguments exited 0" >&2
        failed=1
    else
        echo "ok usage"
    fi
    return "$failed"
}

if [[ ${1:-} == --self-test && $# -eq 1 ]]; then
    self_test
elif [[ $# -eq 1 && -d $1 ]]; then
    compare "$1"
else
    echo "usage: compare-replay-hashes.sh <hashes-dir> | --self-test" >&2
    exit 2
fi
