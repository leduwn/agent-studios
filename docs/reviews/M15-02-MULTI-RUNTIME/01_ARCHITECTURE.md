# M15-02 Architecture — authoritative supplement

Base: PR #26, `a5dbbfc5ebada254727db9a788c0ee40e3035ea8`. This milestone replaces the daemon's singleton routing with the existing `external-runtime::RuntimeRegistry`, not a separate duplicate registry. The builder accepts `with_runtime`, `with_runtimes`, or `with_runtime_registry`; duplicate implementation IDs reject startup. Without explicit adapters, the existing OpenCode builder path remains in place.

`RuntimeRegistry` owns `Arc<dyn AgentRuntime>` adapters in a `BTreeMap<RuntimeImplementationId, ...>`. Discovery snapshots adapters and drops its registry lock before awaiting. Each reported instance must belong to the implementation that discovered it; duplicate instance IDs are removed from resolution and attributed as discovery failures.

`connection.rs` resolves task runtime and instance before creating the physical worktree and starting the runtime. After start, `active_runtime_sessions` keeps the runtime pointer and `RuntimeSessionRef` together; approval records likewise retain the original runtime pointer. A mutable registry can never retarget an already-started session.

**Real adapter inventory:** OpenCode exists and remains the production adapter path; Claude Code and Codex native adapters are NOT IMPLEMENTED unless separately verified against the repository and lifecycle/auth contracts. Mock runtimes exist ONLY in hermetic tests. A registered ID is not evidence that the associated binary, account or model is usable.

Detailed earlier analysis: `04_MULTI_RUNTIME_REGISTRY_ARCHITECTURE.md` and `07_SESSION_OWNERSHIP_AND_LIFECYCLE_INVARIANTS.md`. These supplements take precedence when the prior prose treats unimplemented adapters as available.
