# M15-02 to Claude 2 — exact backend contract and safe UI behavior

Target UI: PR #27. Do not modify the UI worktree from the backend branch.

**Request shape:** `IpcCommand::SubmitTask` is `{"type":"submit_task","studio_id":"<uuid>","task_spec":{...}}` (NEVER `spec`). `task_spec` is a `BatchTaskSpec` with required `key`, `title`, `description`, `assigned_agent_id`, `parent_task_key`, `parent_task_id`; dependency arrays default empty. `task_spec.execution` is optional and holds `runtime_preference`, `runtime_instance_id`, `provider_instance_id`, `model_id`, `repository_path`, `initial_prompt`. **There is NO `model_ref` field in the M15-01 task execution wire shape.**

**Readiness golden shape (synthetic fake-runtime fixture, NOT installed support):**
~~~json
{
  "type": "execution_readiness",
  "readiness": {
    "is_ready": false,
    "registered_runtimes": ["fake-alpha", "fake-beta"],
    "available_runtimes": ["fake-alpha"],
    "runtime_capabilities": {
      "rt-inst-00000000-0000-0000-0000-000000000001": {
        "streaming_events": "supported", "stop": "supported",
        "approvals": "unknown", "model_selection": "unknown"
      }
    },
    "runtime_instances": [
      {"instance_id":"rt-inst-00000000-0000-0000-0000-000000000001","implementation":"fake-alpha","healthy":true},
      {"instance_id":"rt-inst-00000000-0000-0000-0000-000000000002","implementation":"fake-beta","healthy":false}
    ],
    "available_models": [],
    "authorized_workspaces": [],
    "defaults": {},
    "blocking_reasons": ["No models available in provider catalog"]
  }
}
~~~
The sample is an illustrative subset of capability names and blockers, NOT a byte-for-byte output snapshot. Source of truth: `ipc-protocol/src/readiness.rs` and `ipc-transport/tests/golden_fixture_tests.rs`.

**Effective runtime:** read `RunRecord.execution.runtime_preference` AND `runtime_instance_id` after session start. Request intent is not proof of effective selection. Pair `task_id`, `run_id`, `studio_id` from typed events and authoritative RunRecord; do not join events by model or prompt text.

**Routing failure:** asynchronous execution may mark task/run `failed`; a successful `submit_task` command can precede failure. Display returned state and genuine backend error/event, never a fabricated runnable option. Unknown adapter, unhealthy instance, preference mismatch, duplicate identity, discovery failure without explicit route and ambiguous defaults must all fail closed.

**Actual support:** OpenCode implementation is present. Claude Code and Codex native runtime adapters are NOT IMPLEMENTED/NOT VERIFIED; do NOT enable their run button or selectors as if healthy just because a model/provider appears in the catalog. The previous illustrative `03_HANDOFF_AND_DRAFT_PR_NOTICE.md` sample uses an invalid `model_ref` key; use this corrected contract instead.
