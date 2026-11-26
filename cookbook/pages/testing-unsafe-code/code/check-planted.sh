#!/bin/sh
# Checks the checks: every planted bug must be reported, for the expected reason. A plain "the command failed" is not enough,
# because a typo or a broken toolchain also fails.
cd "$(dirname "$0")" || exit 2
status=0
expect() { # feature, test binary, test name, text that the output must contain
    out=$(cargo +nightly miri test --features "$1" --test "$2" -- --exact "$3" 2>&1)
    if [ $? -ne 0 ] && printf '%s' "$out" | grep -qF "$4"; then
        echo "caught   $1: $3"
    else
        echo "MISSED   $1: $3 (wanted: $4)"
        status=1
    fi
}
expect planted-lying-len hostile iterator_that_claims_too_little "slice::get_unchecked requires that the index is within the slice"
expect planted-stale-index hostile closure_that_pushes_and_keeps_the_reference "Undefined Behavior: trying to retag"
expect planted-early-len hostile closure_that_panics_half_way "memory is uninitialized"
expect planted-wrong-get model honest_random_operations_match_a_vec "contents"
exit $status
