# M15-02 Execution Readiness wire contract

Request: authenticated `{"type":"get_execution_readiness"}`. Response: `{"type":"execution_readiness","readiness":{...}}`.

The seven original fields are unchanged: `is_ready`, `available_runtimes`, `runtime_instances`, `available_models`, `authorized_workspaces`, `defaults`, `blocking_reasons`.

Additive optional fields (serde default, omitted when empty):
- `registered_runtimes: string[]` — IDs installed in daemon registry, irrespective of health.
- `runtime_capabilities: Record<instance_id, Record<capability_name, "supported" | "unsupported" | "unknown">>` — static capability support reported for discovered instances; not a promise of runtime availability.

`available_runtimes` contains ONLY implementations with at least one healthy discovered instance. `runtime_instances[]` contains `{instance_id,implementation,healthy}` for each valid discovered instance, including unhealthy ones. A registration with no discoverable instance appears in `registered_runtimes` but NOT `available_runtimes`. Discovery failures contribute blocking reasons; unknown or conflicting instance identities are never advertised as healthy.

`is_ready` also depends on model catalog/default selection, authorized workspace and a deterministic default runtime instance. Two healthy instances of the SAME implementation are ambiguous without explicit selection. A preferred model/provider is not inferred from catalog display names.

**Privacy:** do not put environment variables, provider tokens, serialized secret references or binary authentication contents in readiness. UI must treat capability `unknown` as unsupported for privileged actions unless later verified.

The new optional fields preserve deserialization compatibility for clients that ignore unknown JSON fields. Integration owners should verify their actual client decoder tolerates additive fields before enabling UI mutations.
