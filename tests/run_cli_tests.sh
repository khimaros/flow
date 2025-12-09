#!/bin/bash
set -e

# CLI Binary path
CLI_BIN="./target/debug/flow-cli"

# build the project
echo "building flow-cli..."
cargo build --bin flow-cli

# test 1: Help command
echo "test 1: Check help command..."
$CLI_BIN --help > /dev/null
echo "help command passed."

# test 2: List nodes
echo "test 2: List nodes..."
OUTPUT=$($CLI_BIN --list-nodes workflows/uuid-echo.json 2>&1)
if [[ "$OUTPUT" == *"echo_3fb69f92"* ]]; then
    echo "list nodes passed."
else
    echo "list nodes failed. Output:"
    echo "$OUTPUT"
    exit 1
fi

# test 3: Run workflow shows node status
echo "test 3: Run workflow (uuid-echo)..."
OUTPUT=$($CLI_BIN -f workflows/uuid-echo.json 2>&1)
if [[ "$OUTPUT" == *"started"* ]] && [[ "$OUTPUT" == *"finished"* ]]; then
    echo "run workflow passed."
else
    echo "run workflow failed. Output:"
    echo "$OUTPUT"
    exit 1
fi

# test 4: Piped input to Read node
echo "test 4: Piped input to Read node..."
OUTPUT=$(echo "piped_test_value" | $CLI_BIN workflows/read-echo.json 2>&1)
if [[ "$OUTPUT" == *"piped_test_value"* ]]; then
    echo "piped input passed."
else
    echo "piped input failed. Output:"
    echo "$OUTPUT"
    exit 1
fi

# test 5: ShellCommand with stdin input
echo "test 5: ShellCommand with stdin input..."
OUTPUT=$($CLI_BIN workflows/shell-stdin.json 2>&1)
if [[ "$OUTPUT" == *"finished"* ]] || [[ "$OUTPUT" == *"cached"* ]]; then
    echo "shellCommand stdin passed."
else
    echo "shellCommand stdin failed. Output:"
    echo "$OUTPUT"
    exit 1
fi

# test 6: Interactive stdin prompt (using echo to auto-respond)
echo "test 6: Interactive stdin prompt..."
OUTPUT=$(echo "interactive_test" | $CLI_BIN workflows/read-echo.json 2>&1)
if [[ "$OUTPUT" == *"interactive_test"* ]]; then
    echo "interactive stdin passed."
else
    echo "interactive stdin failed. Output:"
    echo "$OUTPUT"
    exit 1
fi

# test 7: Quiet mode - Echo node outputs raw string to stdout only
echo "test 7: Quiet mode Echo output..."
OUTPUT=$(echo "quiet_test_value" | $CLI_BIN -q workflows/read-echo.json)
if [[ "$OUTPUT" == "quiet_test_value" ]]; then
    echo "quiet Echo passed."
else
    echo "quiet Echo failed. Expected 'quiet_test_value', got:"
    echo "$OUTPUT"
    exit 1
fi

# test 8: Default mode shows header/started/finished, no verbose details
echo "test 8: Default mode output..."
OUTPUT=$($CLI_BIN -f workflows/uuid-echo.json 2>&1)
if [[ "$OUTPUT" == *"executing workflow"* ]] && [[ "$OUTPUT" == *"started"* ]] && [[ "$OUTPUT" == *"finished"* ]] && [[ "$OUTPUT" == *"completed successfully"* ]] && [[ "$OUTPUT" != *"with inputs"* ]]; then
    echo "default mode passed."
else
    echo "default mode failed. Output:"
    echo "$OUTPUT"
    exit 1
fi

# test 9: RegexpExtract workflow
echo "test 9: RegexpExtract workflow..."
OUTPUT=$($CLI_BIN -f workflows/regexp-extract.json 2>&1)
if [[ "$OUTPUT" == *"finished"* ]] || [[ "$OUTPUT" == *"cached"* ]]; then
    echo "regexpExtract passed."
else
    echo "regexpExtract failed. Output:"
    echo "$OUTPUT"
    exit 1
fi

# test 10: --list-handles flag (exact and glob)
echo "test 10: --list-handles flag..."
OUTPUT=$($CLI_BIN --list-handles echo_3fb69f92 workflows/uuid-echo.json 2>&1)
if [[ "$OUTPUT" == *"message"* ]] && [[ "$OUTPUT" == *"| in"* ]] && [[ "$OUTPUT" == *"| out"* ]]; then
    echo "--list-handles (exact) passed."
else
    echo "--list-handles (exact) failed. Output:"
    echo "$OUTPUT"
    exit 1
fi
OUTPUT=$($CLI_BIN --list-handles '*' workflows/uuid-echo.json 2>&1)
if [[ "$OUTPUT" == *"echo_3fb69f92"* ]] && [[ "$OUTPUT" == *"uuid_2f99f484"* ]] && [[ "$OUTPUT" == *"| in"* ]] && [[ "$OUTPUT" == *"| out"* ]]; then
    echo "--list-handles (glob) passed."
else
    echo "--list-handles (glob) failed. Output:"
    echo "$OUTPUT"
    exit 1
fi

# test 11: --set flag to override node input values
echo "test 11: --set flag override..."
OUTPUT=$($CLI_BIN -q --set read_node/input=set_override_value workflows/read-echo.json)
if [[ "$OUTPUT" == "set_override_value" ]]; then
    echo "--set override passed."
else
    echo "--set override failed. Expected 'set_override_value', got:"
    echo "$OUTPUT"
    exit 1
fi

# test 12: --set flag with multiple overrides
echo "test 12: --set flag multiple overrides..."
OUTPUT=$($CLI_BIN -q --set read_node/input=multi_test -f workflows/read-echo.json)
if [[ "$OUTPUT" == "multi_test" ]]; then
    echo "--set multiple passed."
else
    echo "--set multiple failed. Expected 'multi_test', got:"
    echo "$OUTPUT"
    exit 1
fi

# test 13: --set flag with JSON array
echo "test 13: --set flag with JSON array..."
OUTPUT=$($CLI_BIN -v -f --set 'read_node/input=["a","b","c"]' workflows/read-echo.json 2>&1)
if [[ "$OUTPUT" == *'Array([String("a"), String("b"), String("c")])'* ]]; then
    echo "--set JSON array passed."
else
    echo "--set JSON array failed. Output:"
    echo "$OUTPUT"
    exit 1
fi

# test 14: --set with JSON number
echo "test 14: --set flag with JSON number..."
OUTPUT=$($CLI_BIN -v -f --set 'read_node/input=42' workflows/read-echo.json 2>&1)
if [[ "$OUTPUT" == *'Integer(42)'* ]]; then
    echo "--set JSON number passed."
else
    echo "--set JSON number failed. Output:"
    echo "$OUTPUT"
    exit 1
fi

# test 15: --set flag with JSON boolean
echo "test 15: --set flag with JSON boolean..."
OUTPUT=$($CLI_BIN -v -f --set 'read_node/input=true' workflows/read-echo.json 2>&1)
if [[ "$OUTPUT" == *'Boolean(true)'* ]]; then
    echo "--set JSON boolean passed."
else
    echo "--set JSON boolean failed. Output:"
    echo "$OUTPUT"
    exit 1
fi

# test 16: --stdin flag routes to specific node
echo "test 16: --stdin flag..."
OUTPUT=$(echo "stdin_flag_test" | $CLI_BIN -q --stdin read_node/input workflows/read-echo.json)
if [[ "$OUTPUT" == "stdin_flag_test" ]]; then
    echo "--stdin flag passed."
else
    echo "--stdin flag failed. Expected 'stdin_flag_test', got:"
    echo "$OUTPUT"
    exit 1
fi

# test 17: --stdout flag routes specific output
echo "test 17: --stdout flag..."
OUTPUT=$($CLI_BIN -q --set read_node/input=stdout_test --stdout echo_node/output workflows/read-echo.json)
if [[ "$OUTPUT" == "stdout_test" ]]; then
    echo "--stdout flag passed."
else
    echo "--stdout flag failed. Expected 'stdout_test', got:"
    echo "$OUTPUT"
    exit 1
fi

# test 18: --set takes precedence over --stdin
echo "test 18: --set overrides --stdin..."
OUTPUT=$(echo "from_stdin" | $CLI_BIN -q --set read_node/input=from_set --stdin read_node/input workflows/read-echo.json 2>test18_stderr.tmp)
STDERR=$(cat test18_stderr.tmp)
rm -f test18_stderr.tmp
if [[ "$OUTPUT" == "from_set" ]] && [[ "$STDERR" == *"skipping --stdin target"* ]]; then
    echo "--set overrides --stdin passed."
else
    echo "--set overrides --stdin failed. stdout: '$OUTPUT', stderr: '$STDERR'"
    exit 1
fi

# test 19: Quiet mode suppresses all stderr
echo "test 19: Quiet mode no stderr..."
STDERR_OUTPUT=$($CLI_BIN -q -f workflows/uuid-echo.json 2>&1 1>/dev/null)
if [[ -z "$STDERR_OUTPUT" ]]; then
    echo "quiet mode no stderr passed."
else
    echo "quiet mode no stderr failed. Stderr:"
    echo "$STDERR_OUTPUT"
    exit 1
fi

# test 20: Verbose mode shows full details
echo "test 20: Verbose mode..."
OUTPUT=$($CLI_BIN -v -f workflows/uuid-echo.json 2>&1)
if [[ "$OUTPUT" == *"executing workflow"* ]] && [[ "$OUTPUT" == *"completed successfully"* ]] && [[ "$OUTPUT" == *"with inputs"* ]]; then
    echo "verbose mode passed."
else
    echo "verbose mode failed. Output:"
    echo "$OUTPUT"
    exit 1
fi

# test 21: --save flag persists input values to workflow file
echo "test 21: --save flag..."
SAVE_TMP=$(mktemp /tmp/flow_save_test_XXXXXX.json)
cp workflows/read-echo.json "$SAVE_TMP"
$CLI_BIN -q --save --set read_node/input=saved_value "$SAVE_TMP"
if grep -q '"saved_value"' "$SAVE_TMP"; then
    echo "--save flag passed."
else
    echo "--save flag failed. Saved workflow:"
    cat "$SAVE_TMP"
    exit 1
fi
rm -f "$SAVE_TMP"

# test 22: --list-handles shows saved state output values
echo "test 22: --list-handles shows saved state..."
SAVE_TMP=$(mktemp /tmp/flow_save_test_XXXXXX.json)
SAVE_DIR=$(dirname "$SAVE_TMP")
cp workflows/read-echo.json "$SAVE_TMP"
$CLI_BIN -q --save --set read_node/input=state_test_value "$SAVE_TMP"
OUTPUT=$($CLI_BIN --list-handles echo_node "$SAVE_TMP" 2>&1)
if [[ "$OUTPUT" == *"state_test_value"* ]]; then
    echo "--list-handles saved state passed."
else
    echo "--list-handles saved state failed. Output:"
    echo "$OUTPUT"
    exit 1
fi
rm -f "$SAVE_TMP" "$SAVE_DIR/.state/$(basename "$SAVE_TMP")"
rmdir "$SAVE_DIR/.state" 2>/dev/null || true

echo "all CLI tests passed!"
