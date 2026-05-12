# ROADMAP

```
[@] create a UIComponent::Auto which renders the right thing based on the Value type
[ ] screenshots and screencast of builtin workflows in README
[ ] advanced section in nodes or collapse/expand individual inputs
[ ] Node dependency doctor (eg. install dependencies, or popup)
[ ] single command npx or cargo install
[ ] right click -> collapse/expand
[ ] right click -> rename nodes
[ ] use /upstream/<model>/props endpoint for OAI nodes if available
[ ] ability to mark nodes as "continue on failure" or "Run Workflow (continue)"
[ ] job queue as overlay instead of sidebar
[ ] vertical tabs for sidebar
[ ] meshtastic send node error handling
[ ] tabbed interface for workflows
[ ] merge input/output handle border with highlight border?
[ ] custom triggers: date/time, message received, etc.
[ ] workflow side panel show hidden files
[ ] undo / ctrl-z
[ ] terminal output bottom panel
[ ] command palette
[ ] rewrite some web ui in rust
[ ] allow running multiple nodes from cli
[ ] move to a subprocess architecture for nodes -- each node in a separate process
[ ] consider richer data types (secret, audio, etc.) to reduce ui_component overrides
[ ] flow-remote: cli that executes workflows on a running flow-server
[ ] automagic command line tool node (flags as inputs)

[~] tauri app for linux

[x] allow configure default value for node type input
[x] github actions workflow to build macos release tarballs (arm64 + intel)
[x] consistency pass on node/input/output descriptions
[x] flow-cli --env-example flag emitting .env-style output with per-var descriptions
[x] fix: shift-click force-run clears in-progress blue border on node from prior queued job
[x] bundling script to produce release tarball (flow-<version>-<arch>-<python>.tgz)
[x] add seed input to openai_stt, openai_tts, and openai_tts_voice nodes
[x] add language input to openai_tts and openai_voice_design nodes
[x] openai_llm/openai_tts: temperature defaults to unset, not sent unless explicitly set
[x] fix: node creation should not set input values to defaults (should show as placeholders)
[x] fix: shift-click force run should only skip cache for the target node, not dependents
[x] stable sort for keys in workflow files before writing to disk
[x] support Mistral-style paginated voice list format (items array) in OpenAI_TTS
[x] fix: localStorage quota exceeded crash when node outputs are very large
[x] fix: dynamic options pass env_value fallback for env-backed dependency inputs
[x] allow overriding ENV-backed inputs from the UI
[x] audit nodes for places where Auto ui_component could replace explicit ones
[x] visual issue with add node context menu on scroll
[x] search field in node sidebar
[x] add dotenv support
[x] separate workflow state (outputs) from workflow definitions into .state/ sidecar files
[x] unify temp/saved workflow naming (remove tempWorkflowName, use .temp_ as currentWorkflow)
[x] new node: Flatten (concat a list of lists into a single flat list)
[x] --stdin/--stdout flags for explicit pipe wiring to specific node inputs/outputs
[x] --save flag to persist input values and state after execution
[x] restore in-progress node visual state on page refresh
[x] new user_node: XmlSelect (XPath queries on XML/HTML content)
[x] new user_node: FetchHtmlSelect (declarative: WebFetch -> XmlSelect)
[x] Loop node: add limit input to process only first N items
[x] allow piping into workflow: `fortune | flow-cli <workflow>`
[x] new user_node: FetchRSS (url, limit, from_date -> items/count/markdown)
[x] new user_node: FetchArticle (declarative: WebFetch -> HtmlToMarkdown)
[x] new node: Loop (iterate a user_node / declarative node per item
[x] subgroups with custom input/output exposure
[x] allow specifying input values to flow-cli (--set node_id/input=value)
[x] improve HTTP request error messages to include root cause
[x] streaming partial output propagation through DAG (Phase 1: emit + UI)
[x] streaming partial output propagation through DAG (Phase 2: passthrough propagation)
[x] streaming HTTP (SSE) function for Rhai (http_request_sse)
[x] update OpenAI LLM node to use streaming HTTP
[x] centralized env var resolution for all node types (user value > env > default)
[x] auto-convention FLOW_<NODE>_<INPUT> env vars for all inputs
[x] --list-env CLI flag for env var discoverability
[x] request logging middleware for flow-server
[x] DynamicSelect shows default as placeholder, not pre-filled value
[x] OPENAI_API_BASE env var support for rhai nodes
[x] approachable README covering features, install, builtin workflows
[x] new node: Regexp Extract
[x] pull llamaswap metadata modsi/modso in openai_* usernodes
[x] improve performance of List node with very large (read only) lists
[x] new node: List
[x] list editor widget for LIS types (add/delete/reorder items)
[x] new nodes: Templatize, Join, Split (string operations)
[x] declarative user_nodes in json
[x] categories for nodes
[x] flow-cli "--terse" mode
[x] ShellCommand node: expand glob patterns in args
[x] Read node: remove text input, auto-generate prompt from connected edges
[x] ShellCommand node: add stdin input
[x] flow-cli: prompt via stdin when Read node present and no pipe
[x] webui: prompt dialog for Read nodes before execution
[x] do something better with "Save New Workflow" (eg. "Save As")
[x] new node: audio record/upload
[x] new node: OpenAI compat speech-to-text
[x] new node: OpenAI compat text-to-speech
[x] switch all mouse controls to mirror cardinal/vcvrack/firefox
    [x] scroll for vertical pan, shift scroll for horizontal pan, ctrl scroll for zoom
    [x] middle click drag for pan
    [x] arrow / ctrl + arrow / shift + arrow pan
[x] end-to-end testing of cli
[x] end-to-end testing of frontend
[x] end-to-end testing of manual test scenarios
[x] end-to-end testing of the API
[x] input move existing edge
[x] navigation keyboard shortcuts
[x] keyboard shortcut help overlay
[x] consolidate theme switcher (tri-state button, move somewhere)
[x] disable python and JavaScript by default
[x] add make format target
[x] remove "clear canvas" action, instead "new"
[x] remove "clear canvas" right click action, default to node list
[x] filter nodes by type
[x] edge above nodes mode
[x] indication of queued task
[x] add listen flag to flow-server
[x] add running specific node to cli
[x] 100% offline web serving (no third party fetches)
[x] scripting in typescript
[x] DRY python/rhai scripting support
[x] fix ctrl-c behavior (pyo3 root cause)
[x] split scripting module into multiple files
[x] error toasts should extend downward (newest at top)
[x] delete should do the same as backspace
[x] remove "[Source]" annotation from view source button
[x] node resize snap to grid
[x] ctrl-s to save
[~] ctrl-n for new
[x] right click -> add node
[x] get rid of paste (mock)
[x] output edge-drop to add connected node
[x] job submission queue
[x] bypass nodes
[x] color coded inputs/outputs and edges
[x] view source for scripted nodes
[x] caching for nodes with unchanged inputs
[x] workflow side panel rename
[x] workflow side panel delete
[x] node resize minimum size to inputs + outputs
[x] scripting in rhai
[x] scripting in python
[x] node movement snap to grid
[x] themes: dark/light mode
[x] make the sidebar resizeable
[x] WebFetch node
[x] WebSearch node
[x] Display Markdown node
[x] WebFetch readability mode
[x] Split Web Fetch into Fetch and Readability
[x] Web Readability node
[x] Display Code node (removed, replaced by Display Json)
[x] Display node inputs standardized (Text UIComponent for markdown/code)
[x] Display JSON node
```
