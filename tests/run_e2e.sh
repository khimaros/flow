#!/bin/bash
set -e

# define variables
SERVER_BIN="$(pwd)/target/debug/flow-server"
SERVER_PORT=3001
BASE_URL="http://127.0.0.1:$SERVER_PORT"
# create a temporary directory for data
TEST_DATA_DIR=$(mktemp -d)
PID_FILE="$TEST_DATA_DIR/server.pid"

echo "using temporary data directory: $TEST_DATA_DIR"

# function to cleanup server process and temp dir
cleanup() {
    if [ -f "$PID_FILE" ]; then
        echo "stopping server..."
        PID=$(cat "$PID_FILE")
        if kill -0 "$PID" 2>/dev/null; then
            kill "$PID"
        fi
        rm "$PID_FILE"
    fi
    if [ -d "$TEST_DATA_DIR" ]; then
        echo "cleaning up data directory..."
        rm -rf "$TEST_DATA_DIR"
    fi
}

# trap exit to ensure cleanup
trap cleanup EXIT

# 0. Setup test data
echo "setting up test data..."
mkdir -p "$TEST_DATA_DIR/workflows"
mkdir -p "$TEST_DATA_DIR/generated_assets"
mkdir -p "$TEST_DATA_DIR/user_nodes"
echo "dummy content" > "$TEST_DATA_DIR/generated_assets/test_asset.txt"
# glob expansion test files
touch "$TEST_DATA_DIR/generated_assets/glob_file1.txt"
touch "$TEST_DATA_DIR/generated_assets/glob_file2.txt"
touch "$TEST_DATA_DIR/generated_assets/glob_file3.log"
# copy existing workflows and user nodes to the test data dir
cp workflows/*.json "$TEST_DATA_DIR/workflows/"
find user_nodes -maxdepth 1 -type f -exec cp {} "$TEST_DATA_DIR/user_nodes/" \;
# copy test-only nodes
if [ -d "test_nodes" ]; then
    find test_nodes -maxdepth 1 -type f -exec cp {} "$TEST_DATA_DIR/user_nodes/" \;
fi

# 1. Build the project
echo "building project..."
cargo build --bin flow-server

# 2. Start the server in the background from the test data dir (fail fast if port is taken)
if lsof -i :$SERVER_PORT -sTCP:LISTEN >/dev/null 2>&1; then
    echo "ERROR: port $SERVER_PORT is already in use. kill the stale process first:"
    lsof -i :$SERVER_PORT -sTCP:LISTEN
    exit 1
fi
echo "starting server on port $SERVER_PORT..."
pushd "$TEST_DATA_DIR" > /dev/null
FLOW_ECHO_MESSAGE="env_test_value" $SERVER_BIN --listen 127.0.0.1:$SERVER_PORT &
SERVER_PID=$!
echo $SERVER_PID > "$PID_FILE"

# 3. Wait for server to be ready
echo "waiting for server to be ready..."
MAX_RETRIES=30
for i in $(seq 1 $MAX_RETRIES); do
    if curl -s "$BASE_URL/api/nodes" > /dev/null; then
        echo "server is ready!"
        break
    fi
    if [ $i -eq $MAX_RETRIES ]; then
        echo "server failed to start within time limit."
        exit 1
    fi
    sleep 1
done
popd > /dev/null

# 4. Run hurl tests
echo "running Hurl tests..."
hurl --variable base_url="$BASE_URL" --variable data_dir="$TEST_DATA_DIR" --test tests/api.hurl

# 5. Test streaming partial output via SSE
echo "testing streaming partial output events via SSE..."
SSE_LOG=$(mktemp)
# start SSE listener in background
curl -s -N "$BASE_URL/api/queue/stream" > "$SSE_LOG" 2>/dev/null &
SSE_PID=$!
sleep 1

# submit a streaming workflow using ShellCommand
STREAM_RESPONSE=$(curl -s -X POST "$BASE_URL/api/queue/submit" \
    -H "Content-Type: application/json" \
    -d '{
    "workflow": {
        "nodes": [{
            "id": "sse_stream_test",
            "type": "ShellCommand",
            "position": {"x": 0, "y": 0},
            "size": {"width": 300, "height": 300},
            "inputs": {"command": "bash", "args": ["-c", "for w in aaa bbb ccc; do echo $w; sleep 0.1; done"]},
            "skipCache": true,
            "bypassed": false
        }],
        "edges": [],
        "forceRun": true
    },
    "workflow_name": "test_sse_streaming"
}')

SSE_JOB_ID=$(echo "$STREAM_RESPONSE" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null)

# wait for job completion
for i in $(seq 1 30); do
    SSE_STATUS=$(curl -s "$BASE_URL/api/queue/$SSE_JOB_ID" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status',''))" 2>/dev/null)
    if [ "$SSE_STATUS" = "completed" ] || [ "$SSE_STATUS" = "error" ]; then
        break
    fi
    sleep 1
done
sleep 1

# stop SSE listener
kill "$SSE_PID" 2>/dev/null || true
wait "$SSE_PID" 2>/dev/null || true

# verify partial output events appeared in SSE stream
PARTIAL_COUNT=$(grep -c "NodePartialOutput" "$SSE_LOG" 2>/dev/null || echo "0")
if [ "$PARTIAL_COUNT" -lt 2 ]; then
    echo "FAIL: expected at least 2 NodePartialOutput SSE events, got $PARTIAL_COUNT"
    echo "SSE log:"
    cat "$SSE_LOG"
    rm -f "$SSE_LOG"
    exit 1
fi
echo "  found $PARTIAL_COUNT NodePartialOutput events in SSE stream"

# verify accumulated field is present
if ! grep -q '"accumulated"' "$SSE_LOG" 2>/dev/null; then
    echo "FAIL: no accumulated field found in NodePartialOutput events"
    rm -f "$SSE_LOG"
    exit 1
fi
echo "  accumulated field present in partial output events"
rm -f "$SSE_LOG"
echo "streaming SSE tests passed!"

# 6. Test that cancelling a blocking script node completes promptly
echo "testing script node cancellation latency..."
CANCEL_RESPONSE=$(curl -s -X POST "$BASE_URL/api/queue/submit" \
    -H "Content-Type: application/json" \
    -d '{
    "workflow": {
        "nodes": [{
            "id": "slow_cancel_test",
            "type": "SlowCancel",
            "position": {"x": 0, "y": 0},
            "size": {"width": 300, "height": 300},
            "inputs": {"duration_ms": "30000"},
            "skipCache": true,
            "bypassed": false
        }],
        "edges": [],
        "forceRun": true
    },
    "workflow_name": "test_cancel_latency"
}')
CANCEL_JOB_ID=$(echo "$CANCEL_RESPONSE" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null)

# wait for the node to start executing
for i in $(seq 1 30); do
    CANCEL_STATUS=$(curl -s "$BASE_URL/api/queue/$CANCEL_JOB_ID" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status',''))" 2>/dev/null)
    if [ "$CANCEL_STATUS" = "running" ]; then
        break
    fi
    sleep 0.2
done

# cancel the job and measure how long until it's fully done
CANCEL_START=$(date +%s)
curl -s -X POST "$BASE_URL/api/queue/$CANCEL_JOB_ID/cancel" > /dev/null

for i in $(seq 1 60); do
    CANCEL_STATUS=$(curl -s "$BASE_URL/api/queue/$CANCEL_JOB_ID" | python3 -c "import sys,json; j=json.load(sys.stdin); print(j.get('status',''), j.get('completed_at') or '')" 2>/dev/null)
    CANCEL_STATE=$(echo "$CANCEL_STATUS" | awk '{print $1}')
    CANCEL_COMPLETED=$(echo "$CANCEL_STATUS" | awk '{print $2}')
    if [ "$CANCEL_STATE" = "cancelled" ] && [ -n "$CANCEL_COMPLETED" ]; then
        break
    fi
    sleep 0.5
done

CANCEL_END=$(date +%s)
CANCEL_ELAPSED=$((CANCEL_END - CANCEL_START))

if [ "$CANCEL_STATE" != "cancelled" ]; then
    echo "FAIL: job was not cancelled (status: $CANCEL_STATE)"
    exit 1
fi

MAX_CANCEL_SECS=5
if [ "$CANCEL_ELAPSED" -gt "$MAX_CANCEL_SECS" ]; then
    echo "FAIL: cancellation took ${CANCEL_ELAPSED}s (max ${MAX_CANCEL_SECS}s)"
    exit 1
fi
echo "  cancellation completed in ${CANCEL_ELAPSED}s (max ${MAX_CANCEL_SECS}s)"

# verify the queue is not blocked by the orphaned blocking thread:
# submit a follow-up job and check it completes within the time limit
FOLLOWUP_RESPONSE=$(curl -s -X POST "$BASE_URL/api/queue/submit" \
    -H "Content-Type: application/json" \
    -d '{
    "workflow": {
        "nodes": [{
            "id": "followup_echo",
            "type": "Echo",
            "position": {"x": 0, "y": 0},
            "size": {"width": 300, "height": 300},
            "inputs": {"message": "after cancel"},
            "skipCache": true,
            "bypassed": false
        }],
        "edges": [],
        "forceRun": true
    },
    "workflow_name": "test_cancel_followup"
}')
FOLLOWUP_JOB_ID=$(echo "$FOLLOWUP_RESPONSE" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null)

FOLLOWUP_START=$(date +%s)
for i in $(seq 1 60); do
    FOLLOWUP_STATUS=$(curl -s "$BASE_URL/api/queue/$FOLLOWUP_JOB_ID" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status',''))" 2>/dev/null)
    if [ "$FOLLOWUP_STATUS" = "completed" ]; then
        break
    fi
    sleep 0.5
done
FOLLOWUP_END=$(date +%s)
FOLLOWUP_ELAPSED=$((FOLLOWUP_END - FOLLOWUP_START))

if [ "$FOLLOWUP_STATUS" != "completed" ]; then
    echo "FAIL: follow-up job did not complete (status: $FOLLOWUP_STATUS)"
    exit 1
fi
if [ "$FOLLOWUP_ELAPSED" -gt "$MAX_CANCEL_SECS" ]; then
    echo "FAIL: follow-up job took ${FOLLOWUP_ELAPSED}s, queue was blocked by cancelled job (max ${MAX_CANCEL_SECS}s)"
    exit 1
fi
echo "  follow-up job completed in ${FOLLOWUP_ELAPSED}s (queue not blocked)"
echo "script cancellation latency test passed!"

echo "all tests passed!"