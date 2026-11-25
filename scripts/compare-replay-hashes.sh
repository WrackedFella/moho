#!/usr/bin/env bash
# Compare the replay state hashes the three CI platforms wrote.
#
#   compare-replay-hashes.sh <hashes-dir> [platform...]
#   compare-replay-hashes.sh --self-test
#
# <hashes-dir> holds one `<platform>/hashes.txt` per platform, each line
# `<tick> <hash:016x>`. The platforms expected are the arguments, by default
# ubuntu-latest macos-latest windows-latest; one with no or an empty hashes.txt is
# reported as missing and the rest are compared. Prints a Markdown summary on stdout:
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
#
# Runs on the macOS gate too: bash 3.2 and BSD tools, so no associative arrays, mapfile
# or GNU-only flags; the comparison is one POSIX awk program.
set -euo pipefail

DEFAULT_PLATFORMS='ubuntu-latest macos-latest windows-latest'

# A platform without a tick counts as differing from one that has it.
COMPARE_AWK='
BEGIN { n = split(names, P, " ") }
{ sub(/\r$/, "") }
FNR == 1 { idx++ }
NF >= 2 {
    h[idx, $1] = $2
    if (!($1 in seen)) { seen[$1] = 1; ticks[++nt] = $1 }
    ft[idx] = $1
    fh[idx] = $2
}
function val(i, t) { return ((i, t) in h) ? h[i, t] : "-" }
END {
    for (i = 2; i <= nt; i++) {
        t = ticks[i]
        for (j = i - 1; j >= 1 && ticks[j] + 0 > t + 0; j--) ticks[j + 1] = ticks[j]
        ticks[j + 1] = t
    }
    first_bad = ""
    for (k = 1; k <= nt && first_bad == ""; k++)
        for (i = 2; i <= n; i++)
            if (val(i, ticks[k]) != val(1, ticks[k])) { first_bad = ticks[k]; break }

    print "## Replay state hashes"
    print ""
    print "| Platform | Final tick | Final-tick hash |"
    print "|---|---|---|"
    for (i = 1; i <= n; i++) print "| " P[i] " | " ft[i] " | " fh[i] " |"
    print ""
    agree = (first_bad == "") ? "yes" : "no"
    print "agree: " agree
    print "missing: " missing
    if (first_bad != "") {
        print ""
        print "first disagreement: tick " first_bad
        for (i = 1; i <= n; i++) {
            v = ((i, first_bad) in h) ? h[i, first_bad] : "(none)"
            if (v in printed) continue
            printed[v] = 1
            members = ""
            for (j = 1; j <= n; j++) {
                w = ((j, first_bad) in h) ? h[j, first_bad] : "(none)"
                if (w == v) members = members (members == "" ? "" : ", ") P[j]
            }
            print "- " v ": " members
        }
    }
}'

compare() {
    local dir=$1 platform sorted present='' missing=''
    shift
    if [ $# -eq 0 ]; then
        # shellcheck disable=SC2086 # the default is a space-separated list
        set -- $DEFAULT_PLATFORMS
    fi
    sorted=$(printf '%s\n' "$@" | sort)
    set --
    while read -r platform; do
        if [ -s "$dir/$platform/hashes.txt" ]; then
            present="$present $platform"
            set -- "$@" "$dir/$platform/hashes.txt"
        else
            missing="$missing, $platform"
        fi
    done <<< "$sorted"
    missing=${missing#, }
    # /dev/null keeps awk off stdin when no platform has hashes.
    awk -v names="$present" -v missing="${missing:-none}" "$COMPARE_AWK" "$@" /dev/null
}

self_test() {
    local fixtures failed=0 case_dir name actual
    fixtures="$(dirname "${BASH_SOURCE[0]}")/fixtures/replay-hashes"
    for name in agree disagree missing truncated none; do
        case_dir=$fixtures/$name
        if ! actual=$(compare "$case_dir"); then
            echo "FAIL $name: compare exited non-zero" >&2
            failed=1
        elif ! diff <(printf '%s\n' "$actual") <(tr -d '\r' < "$case_dir/expected.md"); then
            echo "FAIL $name: output differs from expected.md" >&2
            failed=1
        else
            echo "ok $name"
        fi
    done
    case_dir=$fixtures/agree
    if ! actual=$(compare "$case_dir" ubuntu-latest windows-latest); then
        echo "FAIL platform-list: compare exited non-zero" >&2
        failed=1
    elif ! diff <(printf '%s\n' "$actual") <(tr -d '\r' < "$case_dir/expected-ubuntu-windows.md"); then
        echo "FAIL platform-list: output differs from expected-ubuntu-windows.md" >&2
        failed=1
    else
        echo "ok platform-list"
    fi
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
elif [[ $# -ge 1 && -d $1 ]]; then
    compare "$@"
else
    echo "usage: compare-replay-hashes.sh <hashes-dir> [platform...] | --self-test" >&2
    exit 2
fi
