#!/usr/bin/env bash
# runs all non-temp workflows with flow-cli.
# all extra arguments are passed through to flow-cli.
#
# examples:
#   ./scripts/run-all-workflows.sh --save -f
#   ./scripts/run-all-workflows.sh --save
set -euo pipefail

cd "$(dirname "$0")/.."

CLI_BIN="${FLOW_CLI:-./target/debug/flow-cli}"

passed=0
failed=0
failures=()

for wf in workflows/*.json; do
    name=$(basename "$wf" .json)

    # skip temp files
    if [[ "$name" == .temp_* ]]; then
        continue
    fi

    echo -n "RUN   $name ... "
    if $CLI_BIN -q "$@" "$wf" 2>/dev/null; then
        echo "OK"
        passed=$((passed + 1))
    else
        echo "FAIL"
        failed=$((failed + 1))
        failures+=("$name")
    fi
done

echo ""
echo "$passed passed, $failed failed"
if [[ $failed -gt 0 ]]; then
    echo "failures: ${failures[*]}"
    exit 1
fi
