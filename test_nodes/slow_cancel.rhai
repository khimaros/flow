// test node that blocks for a long time to verify prompt cancellation
fn spec() {
    return #{
        name: "SlowCancel",
        title: "Slow Cancel Test",
        category: "Test",
        description: "Blocks for a configurable duration to test cancellation latency.",
        inputs: [
            #{
                name: "duration_ms",
                "type": "string",
                ui: "text",
                "default": "30000",
                description: "How long to block (milliseconds)"
            }
        ],
        outputs: [
            #{
                name: "result",
                "type": "string"
            }
        ]
    };
}

fn execute(inputs) {
    let ms = parse_int(inputs["duration_ms"]);
    sleep(ms);
    return #{
        result: "done"
    };
}
