# Release Packaging Report: M14 Mission Control Workbench

**Milestone:** M14 Mission Control Workbench  
**Run ID:** RUN-20261030-M14-MISSION-CONTROL  
**Date:** 2026-10-30  

---

## 1. Package Specification & Build Verification

The VS Code extension package (`.vsix`) was compiled and assembled using `@vscode/vsce package --no-dependencies`:

```bash
> agent-studios-vscode@0.1.0 package
> vsce package --no-dependencies

Executing prepublish script 'npm run vscode:prepublish'...
> agent-studios-vscode@0.1.0 build
> tsc -p ./
```

### 1.1 VSIX Artifact Details
- **File Name:** `agent-studios-vscode-0.1.0.vsix`
- **Output Path:** `apps/agent-studios-vscode/agent-studios-vscode-0.1.0.vsix`
- **Package Size:** **174.64 KB**
- **Total Files Packaged:** **69 files**
- **External Dependencies:** **0** (Production package has zero runtime dependencies; all logic bundled and self-contained).

---

## 2. Packaged File Hierarchy Manifest

```
agent-studios-vscode-0.1.0.vsix
├── [Content_Types].xml
├── extension.vsixmanifest
└── extension/
    ├── LICENSE.txt (10.87 KB)
    ├── package.json (8.06 KB)
    ├── readme.md (6.81 KB)
    ├── media/
    │   └── agents-activitybar.svg (0.46 KB)
    └── out/
        ├── extension.js (10.01 KB)
        ├── bridge/ (13 files, 78.84 KB)
        │   ├── AgentStudiosBridge.js
        │   ├── BridgeManager.js
        │   ├── DisconnectedBridge.js
        │   ├── FixtureBridge.js
        │   ├── IpcBridge.js
        │   ├── IpcFrameCodec.js
        │   ├── MockBridge.js
        │   ├── RustIpcCodec.js
        │   └── types.js
        ├── cards/ (4 files, 41.30 KB)
        │   ├── ApprovalCard.js
        │   ├── DiffReviewManager.js
        │   ├── TaskProgressCard.js
        │   └── ToolCallCard.js
        ├── chat/ (5 files, 97.48 KB)
        │   ├── ChatHtmlBuilder.js
        │   ├── ChatSecurityGuards.js
        │   ├── ChatWebviewViewProvider.js
        │   └── SlashCommandRegistry.js
        ├── commands/ (3 files, 23.31 KB)
        │   ├── ApprovalCommands.js
        │   ├── ArtifactCommands.js
        │   └── DaemonCommands.js
        ├── composer/ (2 files, 30.49 KB)
        │   └── PromptComposer.js
        ├── controls/ (2 files, 24.93 KB)
        │   ├── ModeSelector.js
        │   └── ProviderModelSelector.js
        ├── mission-control/ (21 files, 145.82 KB)
        │   ├── index.js
        │   ├── MissionControlCommands.js
        │   ├── MissionControlHtmlBuilder.js
        │   ├── MissionControlPanel.js
        │   ├── components/
        │   │   ├── AgentInspector.js
        │   │   ├── AttentionQueue.js
        │   │   ├── DagGraphRenderer.js
        │   │   └── KanbanBoard.js
        │   ├── fixtures/
        │   │   └── missionControlFixtures.js
        │   ├── models/
        │   │   └── types.js
        │   ├── state/
        │   │   └── MissionControlProjection.js
        │   ├── styles/
        │   │   └── missionControlStyles.js
        │   └── views/
        │       ├── AgentsView.js
        │       ├── ApprovalsView.js
        │       ├── DiagnosticsView.js
        │       ├── OverviewView.js
        │       ├── ProvidersView.js
        │       ├── ReviewView.js
        │       ├── RunsView.js
        │       ├── TasksView.js
        │       └── TimelineView.js
        ├── state/ (1 file, 16.12 KB)
        │   └── SessionStateProjection.js
        ├── views/ (7 files, 48.52 KB)
        │   ├── ApprovalsTreeProvider.js
        │   ├── ArtifactsTreeProvider.js
        │   ├── ProvidersTreeProvider.js
        │   ├── SessionTreeProvider.js
        │   ├── TasksTreeProvider.js
        │   └── TimelineTreeProvider.js
        └── webviews/ (4 files, 82.03 KB)
            ├── DiffDocumentProvider.js
            ├── graphLayout.js
            ├── SessionDetailWebview.js
            └── TaskGraphWebview.js
```

---

## 3. Deployment & Installation Instructions

### 3.1 Local Installation in VS Code / Code-OSS
To install the compiled VSIX into any VS Code or Code-OSS environment:
```bash
code --install-extension apps/agent-studios-vscode/agent-studios-vscode-0.1.0.vsix
```

### 3.2 Verification Steps Post-Install
1. Launch VS Code / Code-OSS.
2. Open Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`).
3. Run `Agent Studios: Open Mission Control Workbench`.
4. Confirm Mission Control editor tab opens with all 9 tabs functioning and responsive.
