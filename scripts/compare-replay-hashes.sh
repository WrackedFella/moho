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
set -euo pipefail

DEFAULT_PLATFORMS=(ubuntu-latest macos-latest windows-latest)

compare() {
    local dir=$1 platform tick line
    shift
    local -a expected=("$@") present=() missing=() ticks=()
    local -A hash=() seen=() final_tick=() final_hash=()
    if ((${#expected[@]} == 0)); then
        expected=("${DEFAULT_PLATFORMS[@]}")
    fi
    mapfile -t expected < <(printf '%s\n' "${expected[@]}" | sort)

    for platform in "${expected[@]}"; do
        if [[ -s $dir/$platform/hashes.txt ]]; then
            present+=("$platform")
            while read -r tick line; do
                hash[$platform,$tick]=$line
                seen[$tick]=1
                final_tick[$platform]=$tick
                final_hash[$platform]=$line
            done < "$dir/$platform/hashes.txt"
        else
            missing+=("$platform")
        fi
    done
    if ((${#seen[@]})); then
        mapfile -t ticks < <(printf '%s\n' "${!seen[@]}" | sort -n)
    fi

    # A platform without the tick counts as differing from one that has it.
    local first_bad='' reference=${present[0]:-}
    for tick in "${ticks[@]}"; do
        for platform in "${present[@]:1}"; do
            if [[ ${hash[$platform,$tick]:--} != "${hash[$reference,$tick]:--}" ]]; then
                first_bad=$tick
                break 2
            fi
        done
    done

    echo '## Replay state hashes'
    echo
    echo '| Platform | Final tick | Final-tick hash |'
    echo '|---|---|---|'
    for platform in "${present[@]}"; do
        echo "| $platform | ${final_tick[$platform]} | ${final_hash[$platform]} |"
    done
    echo
    if [[ -z $first_bad ]]; then
        echo 'agree: yes'
    else
        echo 'agree: no'
    fi
    if ((${#missing[@]})); then
        local names
        printf -v names '%s, ' "${missing[@]}"
        echo "missing: ${names%, }"
    else
        echo 'missing: none'
    fi
    if [[ -n $first_bad ]]; then
        echo
        echo "first disagreement: tick $first_bad"
        local -A printed=()
        local h members other
        for platform in "${present[@]}"; do
            h=${hash[$platform,$first_bad]:-(none)}
            [[ -z ${printed[$h]:-} ]] || continue
            printed[$h]=1
            members=''
            for other in "${present[@]}"; do
                if [[ ${hash[$other,$first_bad]:-(none)} == "$h" ]]; then
                    members+="${members:+, }$other"
                fi
            done
            echo "- $h: $members"
        done
    fi
}

self_test() {
    local fixtures failed=0 case_dir name actual
    fixtures="$(dirname "${BASH_SOURCE[0]}")/fixtures/replay-hashes"
    for name in agree disagree missing truncated none; do
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
    case_dir=$fixtures/agree
    if ! actual=$(compare "$case_dir" ubuntu-latest windows-latest); then
        echo "FAIL platform-list: compare exited non-zero" >&2
        failed=1
    elif ! diff <(printf '%s\n' "$actual") "$case_dir/expected-ubuntu-windows.md"; then
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
