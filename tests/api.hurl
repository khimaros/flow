# 1. List nodes
GET {{base_url}}/api/nodes
HTTP 200
[Asserts]
jsonpath "$" isCollection
jsonpath "$[0].name" exists

# 2. List workflows
GET {{base_url}}/api/workflows
HTTP 200
[Asserts]
jsonpath "$" isCollection

# 3. Save a new test workflow
POST {{base_url}}/api/workflows/test_workflow
Content-Type: application/json
{
  "nodes": [
    {
      "id": "echo_3fb69f92",
      "type": "Echo",
      "position": {
        "x": 570.0,
        "y": 330.0
      },
      "size": {
        "width": 375.0,
        "height": 210.0
      },
      "inputs": {
        "message": "f11b7285-6598-4aad-ac91-0c9fba49761a"
      },
      "skipCache": false,
      "bypassed": false
    },
    {
      "id": "uuid_2f99f484",
      "type": "UUID",
      "position": {
        "x": 255.0,
        "y": 330.0
      },
      "size": {
        "width": 330.0,
        "height": 105.0
      },
      "inputs": {},
      "skipCache": true,
      "bypassed": false
    }
  ],
  "edges": [
    {
      "id": "reactflow__edge-uuid_2f99f484uuid-echo_3fb69f92message",
      "source": "uuid_2f99f484",
      "sourceHandle": "uuid",
      "target": "echo_3fb69f92",
      "targetHandle": "message"
    }
  ],
  "forceRun": false,
  "targetNodeId": null
}

HTTP 200
[Asserts]
jsonpath "$.status" == "ok"

# 4. Load the test workflow
GET {{base_url}}/api/workflows/test_workflow
HTTP 200
[Asserts]
jsonpath "$.nodes" isCollection
jsonpath "$.nodes[0].type" exists

# 5. Get node options
POST {{base_url}}/api/workflows/test_workflow/nodes/echo_3fb69f92/options/message
Content-Type: application/json
{
  "inputs": {}
}
HTTP 200
[Asserts]
jsonpath "$" isCollection

# 6. Submit a job
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "echo_3fb69f92",
        "type": "Echo",
        "position": {
          "x": 570.0,
          "y": 330.0
        },
        "size": {
          "width": 375.0,
          "height": 210.0
        },
        "inputs": {
          "message": "f11b7285-6598-4aad-ac91-0c9fba49761a"
        },
        "skipCache": false,
        "bypassed": false
      },
      {
        "id": "uuid_2f99f484",
        "type": "UUID",
        "position": {
          "x": 255.0,
          "y": 330.0
        },
        "size": {
          "width": 330.0,
          "height": 105.0
        },
        "inputs": {},
        "skipCache": true,
        "bypassed": false
      }
    ],
    "edges": [
      {
        "id": "reactflow__edge-uuid_2f99f484uuid-echo_3fb69f92message",
        "source": "uuid_2f99f484",
        "sourceHandle": "uuid",
        "target": "echo_3fb69f92",
        "targetHandle": "message"
      }
    ],
    "forceRun": false,
    "targetNodeId": null
  },
  "workflow_name": "test_workflow"
}

HTTP 200
[Asserts]
jsonpath "$.id" exists
jsonpath "$.status" matches /queued|running|completed/
[Captures]
job_id: jsonpath "$.id"

# 7. Get job status
GET {{base_url}}/api/queue/{{job_id}}
HTTP 200
[Asserts]
jsonpath "$.id" == "{{job_id}}"

# 8. List jobs
GET {{base_url}}/api/queue
HTTP 200
[Asserts]
jsonpath "$" isCollection

# 9. Cancel job
POST {{base_url}}/api/queue/{{job_id}}/cancel
HTTP 200

# 10. Rename workflow
POST {{base_url}}/api/workflows/test_workflow/rename
Content-Type: application/json
{
  "new_name": "test_workflow_renamed"
}
HTTP 200
[Asserts]
jsonpath "$.status" == "ok"
jsonpath "$.new_name" == "test_workflow_renamed"

# 11. Delete workflow
POST {{base_url}}/api/workflows/test_workflow_renamed/delete
HTTP 200
[Asserts]
jsonpath "$.status" == "ok"

# 12. Clear completed jobs
DELETE {{base_url}}/api/queue
HTTP 200
[Asserts]
jsonpath "$.status" == "ok"

# 13. Fetch generated asset
GET {{base_url}}/api/assets/test_asset.txt
HTTP 200
[Asserts]
body == "dummy content\n"

# 14. Submit sleep job (to test cancellation reliably)
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "shellcommand_b9124e3f",
        "type": "ShellCommand",
        "position": {
          "x": 330.0,
          "y": 180.0
        },
        "size": {
          "width": 330.0,
          "height": 315.0
        },
        "inputs": {
          "argv": ["sleep", "5"]
        },
        "skipCache": true,
        "bypassed": false
      }
    ],
    "edges": [],
    "forceRun": false,
    "targetNodeId": null
  },
  "workflow_name": "shell-sleep"
}

HTTP 200
[Asserts]
jsonpath "$.id" exists
[Captures]
sleep_job_id: jsonpath "$.id"

# 15. Verify sleep job is running
GET {{base_url}}/api/queue/{{sleep_job_id}}
HTTP 200
[Asserts]
jsonpath "$.status" matches /queued|running/

# 16. Cancel sleep job
POST {{base_url}}/api/queue/{{sleep_job_id}}/cancel
HTTP 200

# 17. Verify sleep job is cancelled
GET {{base_url}}/api/queue/{{sleep_job_id}}
HTTP 200
[Asserts]
jsonpath "$.status" == "cancelled"

# 18. Submit bypassed node workflow
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "bypass_node",
        "type": "ShellCommand",
        "position": { "x": 0, "y": 0 },
        "size": { "width": 100, "height": 100 },
        "inputs": {
          "argv": ["sleep", "0.1"]
        },
        "skipCache": true,
        "bypassed": true
      }
    ],
    "edges": [],
    "forceRun": false
  },
  "workflow_name": "test_bypass"
}
HTTP 200
[Asserts]
jsonpath "$.id" exists
[Captures]
bypass_job_id: jsonpath "$.id"

# 19. Verify bypassed job completion
GET {{base_url}}/api/queue/{{bypass_job_id}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"
jsonpath "$.completed_nodes" == 1
jsonpath "$.total_nodes" == 1

# 20. Submit caching test workflow (First Run)
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "cache_node",
        "type": "ShellCommand",
        "position": { "x": 0, "y": 0 },
        "size": { "width": 100, "height": 100 },
        "inputs": {
          "command": "echo $RANDOM > {{data_dir}}/generated_assets/cache_test.txt",
          "shell": "bash"
        },
        "skipCache": false,
        "bypassed": false
      }
    ],
    "edges": [],
    "forceRun": false
  },
  "workflow_name": "test_cache"
}
HTTP 200
[Captures]
cache_job_1: jsonpath "$.id"

# 21. Wait for cache job 1
GET {{base_url}}/api/queue/{{cache_job_1}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"

# 22. Fetch first random value
GET {{base_url}}/api/assets/cache_test.txt
HTTP 200
[Asserts]
body matches /[0-9]+/
[Captures]
rand_val_1: body

# 23. Submit caching test workflow (Second Run - Should Cache)
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "cache_node",
        "type": "ShellCommand",
        "position": { "x": 0, "y": 0 },
        "size": { "width": 100, "height": 100 },
        "inputs": {
          "command": "echo $RANDOM > {{data_dir}}/generated_assets/cache_test.txt",
          "shell": "bash"
        },
        "skipCache": false,
        "bypassed": false
      }
    ],
    "edges": [],
    "forceRun": false
  },
  "workflow_name": "test_cache"
}
HTTP 200
[Captures]
cache_job_2: jsonpath "$.id"

# 24. Wait for cache job 2
GET {{base_url}}/api/queue/{{cache_job_2}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"

# 25. Verify file was NOT updated (cached)
GET {{base_url}}/api/assets/cache_test.txt
HTTP 200
[Asserts]
body == "{{rand_val_1}}"

# 26. Submit caching test workflow (Third Run - Force Run)
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "cache_node",
        "type": "ShellCommand",
        "position": { "x": 0, "y": 0 },
        "size": { "width": 100, "height": 100 },
        "inputs": {
          "command": "echo $RANDOM > {{data_dir}}/generated_assets/cache_test.txt",
          "shell": "bash"
        },
        "skipCache": false,
        "bypassed": false
      }
    ],
    "edges": [],
    "forceRun": true
  },
  "workflow_name": "test_cache"
}
HTTP 200
[Captures]
cache_job_3: jsonpath "$.id"

# 27. Wait for cache job 3
GET {{base_url}}/api/queue/{{cache_job_3}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"

# 28. Verify file WAS updated (force run)
GET {{base_url}}/api/assets/cache_test.txt
HTTP 200
[Asserts]
body != "{{rand_val_1}}"

# 29. Submit bypassed node with dependent (Should Fail Dependent)
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "source_node",
        "type": "UUID",
        "position": { "x": 0, "y": 0 },
        "size": { "width": 100, "height": 100 },
        "inputs": {},
        "bypassed": true
      },
      {
        "id": "echo_node",
        "type": "Echo",
        "position": { "x": 200, "y": 0 },
        "size": { "width": 100, "height": 100 },
        "inputs": {
          "message": { "$node": "source_node", "$output": "uuid" }
        },
        "bypassed": false
      }
    ],
    "edges": [
      {
        "id": "edge1",
        "source": "source_node",
        "sourceHandle": "uuid",
        "target": "echo_node",
        "targetHandle": "message"
      }
    ],
    "forceRun": false
  },
  "workflow_name": "test_bypass_dependent"
}
HTTP 200
[Captures]
bypass_dep_job_id: jsonpath "$.id"

# 30. Wait for bypassed dependent job
GET {{base_url}}/api/queue/{{bypass_dep_job_id}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "error"
jsonpath "$.error" contains "missing output from node source_node"

# 31. Submit workflow with chained nodes to test event ordering
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "echo_first",
        "type": "Echo",
        "position": { "x": 0, "y": 0 },
        "size": { "width": 100, "height": 100 },
        "inputs": {
          "message": "hello"
        },
        "skipCache": true,
        "bypassed": false
      },
      {
        "id": "echo_second",
        "type": "Echo",
        "position": { "x": 200, "y": 0 },
        "size": { "width": 100, "height": 100 },
        "inputs": {
          "message": { "$node": "echo_first", "$output": "output" }
        },
        "skipCache": true,
        "bypassed": false
      }
    ],
    "edges": [
      {
        "id": "edge_chain",
        "source": "echo_first",
        "sourceHandle": "output",
        "target": "echo_second",
        "targetHandle": "message"
      }
    ],
    "forceRun": false
  },
  "workflow_name": "test_event_ordering"
}
HTTP 200
[Captures]
event_order_job_id: jsonpath "$.id"

# 32. Wait for event ordering job to complete
GET {{base_url}}/api/queue/{{event_order_job_id}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"
jsonpath "$.completed_nodes" == 2
jsonpath "$.total_nodes" == 2
# active_nodes should be empty after completion
jsonpath "$.active_nodes" isEmpty

# 33. Submit cached workflow to verify cached nodes complete properly
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "echo_cached",
        "type": "Echo",
        "position": { "x": 0, "y": 0 },
        "size": { "width": 100, "height": 100 },
        "inputs": {
          "message": "cached_test_message"
        },
        "skipCache": false,
        "bypassed": false
      }
    ],
    "edges": [],
    "forceRun": false
  },
  "workflow_name": "test_cached_completion"
}
HTTP 200
[Captures]
cached_job_1_id: jsonpath "$.id"

# 34. Wait for first cached job
GET {{base_url}}/api/queue/{{cached_job_1_id}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"

# 35. Re-submit same workflow (should use cache)
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "echo_cached",
        "type": "Echo",
        "position": { "x": 0, "y": 0 },
        "size": { "width": 100, "height": 100 },
        "inputs": {
          "message": "cached_test_message"
        },
        "skipCache": false,
        "bypassed": false
      }
    ],
    "edges": [],
    "forceRun": false
  },
  "workflow_name": "test_cached_completion"
}
HTTP 200
[Captures]
cached_job_2_id: jsonpath "$.id"

# 36. Verify cached job completes (ensures NodeStarted is sent for cached nodes)
GET {{base_url}}/api/queue/{{cached_job_2_id}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"
jsonpath "$.completed_nodes" == 1
jsonpath "$.active_nodes" isEmpty

# 37. Test ShellCommand with stdin input
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "shell_stdin_node",
        "type": "ShellCommand",
        "position": { "x": 0, "y": 0 },
        "size": { "width": 100, "height": 100 },
        "inputs": {
          "command": "cat",
          "stdin": "stdin_test_value"
        },
        "skipCache": true,
        "bypassed": false
      }
    ],
    "edges": [],
    "forceRun": false
  },
  "workflow_name": "test_shell_stdin"
}
HTTP 200
[Captures]
shell_stdin_job_id: jsonpath "$.id"

# 38. Wait for shell stdin job to complete
GET {{base_url}}/api/queue/{{shell_stdin_job_id}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"

# 39. Test Read node exists (no visible inputs, input is injected)
GET {{base_url}}/api/nodes
HTTP 200
[Asserts]
jsonpath "$[?(@.name=='Read')]" count == 1

# 40. Test ShellCommand node has stdin input spec
GET {{base_url}}/api/nodes
HTTP 200
[Asserts]
jsonpath "$[?(@.name=='ShellCommand')].inputs[?(@.name=='stdin')]" exists

# 41. Test ShellCommand glob expansion in args
# uses ls with a glob pattern - if glob expansion works, command succeeds.
# if glob wasn't expanded, ls would receive literal "*.txt" and fail.
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "glob_test",
        "type": "ShellCommand",
        "position": {"x": 0.0, "y": 0.0},
        "size": {"width": 300.0, "height": 300.0},
        "inputs": {
          "argv": ["ls", "{{data_dir}}/generated_assets/*.txt"]
        },
        "skipCache": true,
        "bypassed": false
      }
    ],
    "edges": []
  },
  "workflow_name": "glob-test"
}
HTTP 200
[Asserts]
jsonpath "$.id" exists
[Captures]
glob_job_id: jsonpath "$.id"

# 42. Wait for glob test job - completion proves glob was expanded
GET {{base_url}}/api/queue/{{glob_job_id}}
[Options]
retry: 10
retry-interval: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"

# 43. Verify RegexpExtract node is registered
GET {{base_url}}/api/nodes
HTTP 200
[Asserts]
jsonpath "$[?(@.name=='RegexpExtract')]" count == 1

# 44. Submit RegexpExtract workflow
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "regexp_test",
        "type": "RegexpExtract",
        "position": {"x": 0.0, "y": 0.0},
        "size": {"width": 300.0, "height": 300.0},
        "inputs": {
          "text": "alice@example.com and bob@test.org",
          "pattern": "(?P<user>\\w+)@(?P<domain>\\w+\\.\\w+)",
          "case_sensitive": true
        },
        "skipCache": true,
        "bypassed": false
      }
    ],
    "edges": [],
    "forceRun": false
  },
  "workflow_name": "test_regexp_extract"
}

HTTP 200
[Asserts]
jsonpath "$.id" exists
[Captures]
regexp_job_id: jsonpath "$.id"

# 45. Wait for RegexpExtract job to complete
GET {{base_url}}/api/queue/{{regexp_job_id}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"

# 46. Submit case insensitive RegexpExtract workflow
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "regexp_ci_test",
        "type": "RegexpExtract",
        "position": {"x": 0.0, "y": 0.0},
        "size": {"width": 300.0, "height": 300.0},
        "inputs": {
          "text": "Hello WORLD hello",
          "pattern": "hello",
          "case_sensitive": false
        },
        "skipCache": true,
        "bypassed": false
      }
    ],
    "edges": [],
    "forceRun": false
  },
  "workflow_name": "test_regexp_case_insensitive"
}

HTTP 200
[Asserts]
jsonpath "$.id" exists
[Captures]
regexp_ci_job_id: jsonpath "$.id"

# 47. Wait for case insensitive RegexpExtract job
GET {{base_url}}/api/queue/{{regexp_ci_job_id}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"

# 48. Submit streaming ShellCommand workflow
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "stream_test",
        "type": "ShellCommand",
        "position": {"x": 0.0, "y": 0.0},
        "size": {"width": 300.0, "height": 300.0},
        "inputs": {
          "command": "for w in hello world; do echo $w; sleep 0.05; done"
        },
        "skipCache": true,
        "bypassed": false
      }
    ],
    "edges": [],
    "forceRun": true
  },
  "workflow_name": "test_streaming"
}

HTTP 200
[Asserts]
jsonpath "$.id" exists
[Captures]
stream_job_id: jsonpath "$.id"

# 49. Wait for streaming job to complete
GET {{base_url}}/api/queue/{{stream_job_id}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"
jsonpath "$.completed_nodes" == 1

# 50. Submit streaming workflow with downstream display node
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "stream_src",
        "type": "ShellCommand",
        "position": {"x": 0.0, "y": 0.0},
        "size": {"width": 300.0, "height": 300.0},
        "inputs": {
          "command": "for w in foo bar; do echo $w; sleep 0.05; done"
        },
        "skipCache": true,
        "bypassed": false
      },
      {
        "id": "display_dst",
        "type": "DisplayMarkdown",
        "position": {"x": 400.0, "y": 0.0},
        "size": {"width": 300.0, "height": 300.0},
        "inputs": {
          "markdown": {"$node": "stream_src", "$output": "stdout"}
        },
        "skipCache": true,
        "bypassed": false
      }
    ],
    "edges": [
      {
        "id": "edge_stream",
        "source": "stream_src",
        "sourceHandle": "stdout",
        "target": "display_dst",
        "targetHandle": "markdown"
      }
    ],
    "forceRun": true
  },
  "workflow_name": "test_streaming_propagation"
}

HTTP 200
[Asserts]
jsonpath "$.id" exists
[Captures]
stream_prop_job_id: jsonpath "$.id"

# 51. Wait for streaming propagation job to complete
GET {{base_url}}/api/queue/{{stream_prop_job_id}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"
jsonpath "$.completed_nodes" == 2

# 52. Verify auto env var (FLOW_ECHO_MESSAGE) appears in node metadata
GET {{base_url}}/api/nodes
HTTP 200
[Asserts]
jsonpath "$[?(@.name=='Echo')].inputs[?(@.name=='message')].env_var" nth 0 == "FLOW_ECHO_MESSAGE"
jsonpath "$[?(@.name=='Echo')].inputs[?(@.name=='message')].env_value" nth 0 == "env_test_value"

# 53. Submit Echo workflow with empty message to test env var fallback
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "env_echo",
        "type": "Echo",
        "position": {"x": 0, "y": 0},
        "size": {"width": 300, "height": 300},
        "inputs": {"message": ""},
        "skipCache": true,
        "bypassed": false
      }
    ],
    "edges": [],
    "forceRun": true
  },
  "workflow_name": "test_env_override"
}
HTTP 200
[Captures]
env_job_id: jsonpath "$.id"

# 54. Wait for env override job
GET {{base_url}}/api/queue/{{env_job_id}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"

# 55. Submit Echo with explicit value — should NOT be overridden by env
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "env_echo_explicit",
        "type": "Echo",
        "position": {"x": 0, "y": 0},
        "size": {"width": 300, "height": 300},
        "inputs": {"message": "user_value"},
        "skipCache": true,
        "bypassed": false
      }
    ],
    "edges": [],
    "forceRun": true
  },
  "workflow_name": "test_env_no_override"
}
HTTP 200
[Captures]
env_no_override_job_id: jsonpath "$.id"

# 56. Wait for env no-override job
GET {{base_url}}/api/queue/{{env_no_override_job_id}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"

# 57. DynamicUserNode is registered and flagged for per-instance spec fetching
GET {{base_url}}/api/nodes
HTTP 200
[Asserts]
jsonpath "$[?(@.name=='DynamicUserNode')]" count == 1
jsonpath "$[?(@.name=='DynamicUserNode')].has_dynamic_spec" nth 0 == true

# 58. Save a workflow with a DynamicUserNode wrapping inline declarative JSON
POST {{base_url}}/api/workflows/test_dynamic_user_node
Content-Type: application/json
{
  "nodes": [
    {
      "id": "dyn_node",
      "type": "DynamicUserNode",
      "position": { "x": 0.0, "y": 0.0 },
      "size": { "width": 400.0, "height": 400.0 },
      "inputs": {
        "language": "declarative",
        "code": "{\"name\":\"InlineEcho\",\"title\":\"Inline Echo\",\"category\":\"Test\",\"description\":\"\",\"inputs\":[{\"name\":\"text\",\"mapping\":\"echo.message\"}],\"outputs\":[{\"name\":\"result\",\"mapping\":\"echo.output\"}],\"implementation\":{\"nodes\":[{\"id\":\"echo\",\"type\":\"Echo\",\"position\":{\"x\":0,\"y\":0},\"size\":{\"width\":300,\"height\":200},\"inputs\":{},\"skipCache\":false,\"bypassed\":false}],\"edges\":[]}}",
        "text": "hurl says hello"
      },
      "skipCache": true,
      "bypassed": false
    }
  ],
  "edges": [],
  "forceRun": false,
  "targetNodeId": null
}
HTTP 200
[Asserts]
jsonpath "$.status" == "ok"

# 59. Fetch per-instance spec — dynamic ports should appear alongside the static control inputs
POST {{base_url}}/api/workflows/test_dynamic_user_node/nodes/dyn_node/spec
Content-Type: application/json
{
  "inputs": {}
}
HTTP 200
[Asserts]
jsonpath "$.name" == "DynamicUserNode"
jsonpath "$.has_dynamic_spec" == true
jsonpath "$.inputs[?(@.name=='code')]" count == 1
jsonpath "$.inputs[?(@.name=='language')]" count == 1
jsonpath "$.inputs[?(@.name=='text')]" count == 1
jsonpath "$.outputs[?(@.name=='result')]" count == 1

# 60. Fetch spec with overridden code from the request body
# (simulates editor-time spec preview before saving)
POST {{base_url}}/api/workflows/test_dynamic_user_node/nodes/dyn_node/spec
Content-Type: application/json
{
  "inputs": {
    "language": "declarative",
    "code": "{\"name\":\"InlinePair\",\"title\":\"Inline Pair\",\"category\":\"Test\",\"description\":\"\",\"inputs\":[{\"name\":\"alpha\",\"mapping\":\"echo.message\"},{\"name\":\"beta\",\"mapping\":\"echo.message\"}],\"outputs\":[{\"name\":\"gamma\",\"mapping\":\"echo.output\"}],\"implementation\":{\"nodes\":[{\"id\":\"echo\",\"type\":\"Echo\",\"position\":{\"x\":0,\"y\":0},\"size\":{\"width\":300,\"height\":200},\"inputs\":{},\"skipCache\":false,\"bypassed\":false}],\"edges\":[]}}"
  }
}
HTTP 200
[Asserts]
jsonpath "$.inputs[?(@.name=='alpha')]" count == 1
jsonpath "$.inputs[?(@.name=='beta')]" count == 1
jsonpath "$.outputs[?(@.name=='gamma')]" count == 1
# 'text' from the saved workflow should NOT appear once the editor overrides code
jsonpath "$.inputs[?(@.name=='text')]" count == 0

# 61. Run the saved DynamicUserNode workflow end-to-end via the queue
POST {{base_url}}/api/queue/submit
Content-Type: application/json
{
  "workflow": {
    "nodes": [
      {
        "id": "dyn_node",
        "type": "DynamicUserNode",
        "position": { "x": 0.0, "y": 0.0 },
        "size": { "width": 400.0, "height": 400.0 },
        "inputs": {
          "language": "declarative",
          "code": "{\"name\":\"InlineEcho\",\"title\":\"Inline Echo\",\"category\":\"Test\",\"description\":\"\",\"inputs\":[{\"name\":\"text\",\"mapping\":\"echo.message\"}],\"outputs\":[{\"name\":\"result\",\"mapping\":\"echo.output\"}],\"implementation\":{\"nodes\":[{\"id\":\"echo\",\"type\":\"Echo\",\"position\":{\"x\":0,\"y\":0},\"size\":{\"width\":300,\"height\":200},\"inputs\":{},\"skipCache\":false,\"bypassed\":false}],\"edges\":[]}}",
          "text": "hurl says hello"
        },
        "skipCache": true,
        "bypassed": false
      }
    ],
    "edges": [],
    "forceRun": true
  },
  "workflow_name": "test_dynamic_user_node"
}
HTTP 200
[Captures]
dyn_job_id: jsonpath "$.id"

# 62. Wait for dynamic node job to complete
GET {{base_url}}/api/queue/{{dyn_job_id}}
[Options]
retry: 10
delay: 200
HTTP 200
[Asserts]
jsonpath "$.status" == "completed"
jsonpath "$.completed_nodes" == 1

# 63. Delete the test workflow
POST {{base_url}}/api/workflows/test_dynamic_user_node/delete
HTTP 200
[Asserts]
jsonpath "$.status" == "ok"
