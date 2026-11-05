# M15-02 Registry and deterministic routing

Authoritative input is `BatchTaskSpec.execution: Option<TaskExecutionSpec>`, whose fields are `runtime_preference`, `runtime_instance_id`, `provider_instance_id`, `model_id`, `repository_path`, `initial_prompt`. The submission wire field is `task_spec` (not `spec`).

Resolution is fail-closed:
1. **Implementation + instance:** implementation must be registered; selected instance ID must be discovered, healthy, unambiguous and owned by that implementation.
2. **Instance only:** its discovered ID must have exactly one owner. Unknown, unhealthy, duplicate or forged identities reject execution.
3. **Implementation only:** use an explicitly configured instance only if it belongs to that implementation; otherwise require exactly one healthy instance for that implementation.
4. **Neither:** use the configured default instance if healthy; absent a default, allow only one unambiguous healthy instance and no failed discovery. Incomplete discovery cannot trigger silent substitution.

Model/provider resolution remains the M15-01 catalog contract: provider and model must match; ambiguous model aliases or unknown providers fail, and prompt contents remain unchanged. Physical execution runs from a managed Git worktree, not the source repository.

Routing failures occur asynchronously during the task lifecycle and are reflected by failed task/run states and emitted events. A successful `submit_task` response does NOT prove that runtime execution succeeded. An effective `RunRecord.execution` is populated after session start with actual implementation, instance and resolved model/provider fields. Do not infer effective identity merely from the user's requested selector.

Registry lock is not held while discovering, starting, stopping, sending approval responses or consuming runtime events.
