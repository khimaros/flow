#!/bin/bash
set -e

# define variables
SERVER_BIN="./target/debug/flow-server"
SERVER_PORT=3001
BASE_URL="http://127.0.0.1:$SERVER_PORT"
PID_FILE="server_ui.pid"
# create a temporary directory for data
TEST_DATA_DIR=$(mktemp -d)

echo "using temporary data directory: $TEST_DATA_DIR"

# function to cleanup server process and temp dir
cleanup() {
    echo "cleanup starting..."
    if [ -f "$PID_FILE" ]; then
        echo "stopping server..."
        PID=$(cat "$PID_FILE")
        if kill -0 "$PID" 2>/dev/null; then
            kill "$PID"
            # wait for server to actually terminate
            wait "$PID" 2>/dev/null || true
        fi
        rm -f "$PID_FILE"
    else
        echo "No PID file found at $PID_FILE"
    fi
    if [ -d "$TEST_DATA_DIR" ]; then
        echo "cleaning up data directory..."
        rm -rf "$TEST_DATA_DIR"
    fi
    echo "cleanup complete."
}

# trap exit to ensure cleanup
trap cleanup EXIT

# 0. Setup test data
echo "setting up test data..."
mkdir -p "$TEST_DATA_DIR/workflows"
mkdir -p "$TEST_DATA_DIR/generated_assets"
mkdir -p "$TEST_DATA_DIR/user_nodes"
# copy existing workflows to the test data dir
cp workflows/*.json "$TEST_DATA_DIR/workflows/"
# copy user nodes to the test data dir
find user_nodes -maxdepth 1 -type f -exec cp {} "$TEST_DATA_DIR/user_nodes/" \;

# 1. Build the UI
# echo "building UI..."
# cd ui
# npm install
# # Install playwright browsers (chromium only to save time/space)
# npx playwright install chromium
# npm run build
# cd ..

# 2. Build the server (embeds the UI)
# echo "building server..."
# cargo build --bin flow-server

# 3. Start the server (fail fast if port is taken)
if lsof -i :$SERVER_PORT -sTCP:LISTEN >/dev/null 2>&1; then
    echo "ERROR: port $SERVER_PORT is already in use. kill the stale process first:"
    lsof -i :$SERVER_PORT -sTCP:LISTEN
    exit 1
fi
echo "starting server on port $SERVER_PORT..."
RUST_LOG=debug $SERVER_BIN --listen 127.0.0.1:$SERVER_PORT --data-dir "$TEST_DATA_DIR" > server.log 2>&1 &
SERVER_PID=$!
echo $SERVER_PID > "$PID_FILE"

# 4. Wait for server
echo "waiting for server to be ready..."
MAX_RETRIES=30
for i in $(seq 1 $MAX_RETRIES); do
    if curl -s "$BASE_URL/api/nodes" > /dev/null; then
        echo "server is ready!"
        break
    fi
    if [ $i -eq $MAX_RETRIES ]; then
        echo "server failed to start within time limit."
        cat server.log
        exit 1
    fi
    sleep 1
done

# 5. Run Playwright tests
echo "running Playwright tests..."
cd ui
# don't exit on test failure - we want to clean up
set +e
if [ -n "$1" ]; then
    echo "running tests matching: $1"
    # pass all arguments after the first one to playwright (e.g., --headed, --debug)
    shift_args="${@:2}"
    npx playwright test --grep "$1" $shift_args
else
    npm run test
fi
TEST_EXIT_CODE=$?
set -e
cd ..

if [ $TEST_EXIT_CODE -eq 0 ]; then
    echo "UI Tests passed!"
else
    echo "UI Tests failed with exit code $TEST_EXIT_CODE"
    #echo "=== SERVER LOGS (tail) ==="
    #tail -n 100 server.log
    exit $TEST_EXIT_CODE
fi
