# Agent Studios CLI (`ags`) — Command Surface & CLI Interface Specification

> **Document**: `AGS_CLI_COMMAND_SURFACE.md`  
> **Status**: Canonical CLI & Slash Command Specification  
> **Target Subsystem**: `ags` / Clap CLI Parser / Slash Command Engine  
> **Precedence**: Aligned with `PRODUCT_PRINCIPLES.md` (Principles #7, #8, #21, #26).

---

## 1. CLI Binary Arguments & Subcommands

The `ags` binary provides both an interactive Ratatui TUI and a scriptable non-interactive execution mode using `clap` (derive API).

### 1.1 Usage Synopsis

```text
ags [OPTIONS] [PROMPT]
ags <COMMAND> [OPTIONS]
```

---

### 1.2 Top-Level Options & Flags

| Flag / Option | Short | Value Type | Description |
| :--- | :---: | :--- | :--- |
| `--provider` | `-p` | `STRING` | Override provider instance ID (e.g., `anthropic`, `openai`, `gemini`, `ollama`). |
| `--model` | `-m` | `STRING` | Override model target ID (e.g., `claude-3-7-sonnet`, `o3-mini`, `deepseek-r1:8b`). |
| `--runtime` | `-r` | `STRING` | Select execution runtime: `internal-codex`, `opencode`, `claude-code`. |
| `--session` | `-s` | `STRING` | Resume an existing session by ID or keyword `last`. |
| `--worktree` | `-w` | `PATH` | Execute within a dedicated isolated Git worktree. |
| `--lang` | | `en-US` \| `vi-VN` | Force UI display language (overrides OS locale). |
| `--theme` | | `dark` \| `light` \| `adaptive` | Set visual theme. Default: `dark`. |
| `--no-color` | | `FLAG` | Disable ANSI colors and backgrounds (NO_COLOR standard). |
| `--auto-approve` | `-y` | `none` \| `readonly` \| `workspace` \| `all` | Security approval policy preset. Default: `readonly`. |
| `--open-ide` | | `FLAG` | Simultaneously launch Code-OSS attached to this session. |
| `--verbose` | `-v` | `FLAG` | Enable verbose diagnostics and tracing in logs. |
| `--help` | `-h` | `FLAG` | Print localized help information. |
| `--version` | `-V` | `FLAG` | Print version and build commit hash. |

---

### 1.3 Subcommands

#### `ags run [OPTIONS] <PROMPT>`
Executes a single prompt headlessly without launching the interactive TUI. Streams progress events directly to stdout and outputs the final result with Unix exit code 0 on success.
- Example: `ags run "Run cargo check and fix any compiler warnings"`
- Supports piping: `cat error.log | ags run "Analyze this failure and suggest a fix"`

#### `ags resume [SESSION_ID]`
Resumes an existing session in interactive TUI mode with its complete task graph, conversation history, and allocated worktrees intact.
- Example: `ags resume s_9a8f21` or `ags resume last`

#### `ags sessions`
Lists all recent sessions across the current repository:
```text
ID         BRANCH              STARTED      TASKS  MODEL               STATUS
s_9a8f21   feat/m12-claude*    1 hour ago   3/3    claude-3-7-sonnet   Succeeded
s_7c2e40   fix/token-budget    Yesterday    1/2    o3-mini             Interrupted
s_3d1a89   main                3 days ago   4/4    gemini-2.5-pro      Succeeded
```

#### `ags model [SUBCOMMAND]`
Inspects and manages configured provider models:
- `ags model list`: Lists available models across all active provider instances.
- `ags model set <PROVIDER> <MODEL>`: Sets workspace default model.
- `ags model benchmark`: Runs latency and token throughput tests.

#### `ags runtime [SUBCOMMAND]`
Manages external runtime adapters:
- `ags runtime list`: Shows status of `internal-codex`, `opencode`, and `claude-code`.
- `ags runtime doctor`: Verifies installed binaries, sandboxes, and adapter health.

#### `ags worktree [SUBCOMMAND]`
Inspects isolated Git worktrees allocated by the Control Plane:
- `ags worktree list`: Shows active worktrees, assigned agents, and branch targets.
- `ags worktree clean`: Removes orphaned worktrees whose runs have completed.

#### `ags open-ide [SESSION_ID]`
Launches the Agent Studios Code-OSS desktop IDE and attaches to the target session.

---

## 2. Interactive Slash Commands (Inside TUI Composer)

When working inside the interactive TUI, typing `/` opens the autocompletion overlay.

| Slash Command | Arguments | Description & Behavior |
| :--- | :--- | :--- |
| `/help` or `/?` | `[topic]` | Displays interactive help modal or specific documentation. |
| `/model` | `[provider/model]` | Without args: opens Model Selector popup. With args: switches active model target instantly. |
| `/runtime` | `[runtime_id]` | Switches active execution runtime (`internal-codex`, `opencode`, `claude-code`). |
| `/task` | `<goal>` | Formally submits a high-level goal to the Coordinator to construct a DAG Task Graph. |
| `/tasks` | | Toggles the Task Graph inspector overlay showing all tasks, states, and dependencies. |
| `/worktree` | `[list\|clean]` | Opens the worktree manager modal showing active agent worktrees. |
| `/diff` | `[file]` | Opens the Unified Diff Review modal showing uncommitted modifications. |
| `/patch` | `apply\|discard` | Directly applies or discards the latest staged patch artifact. |
| `/approve` | `always\|reset` | Manages tool execution permissions for the current session. |
| `/compact` | | Triggers manual context compression and summarization. |
| `/clear` | | Clears the visible transcript buffer (retains Control Plane event history). |
| `/ide` | | Launches Agent Studios Code-OSS IDE connected to this shared session (`Ctrl+O`). |
| `/lang` | `en\|vi` | Toggles UI display language between English and Vietnamese (`en-US` / `vi-VN`). |
| `/theme` | `dark\|light\|adaptive`| Switches color theme in real-time. |
| `/status` | | Prints session telemetry, token usage, cost, and Control Plane connection status. |
| `/exit` or `/quit` | | Cleanly exits `ags` TUI (`Ctrl+D`). |

---

## 3. Bilingual Localization & Vietnamese Translation Reference

Per Product Principle #26 and `LOCALIZATION.md`, CLI help strings, diagnostics, and slash commands are fully localized in English and Vietnamese.

### 3.1 CLI Help Reference (`ags --help`)

#### English (`en-US`)
```text
Agent Studios CLI (`ags`) 0.12.0
Deterministic Multi-Agent Development Harness for Software Engineers

USAGE:
    ags [OPTIONS] [PROMPT]
    ags <SUBCOMMAND>

OPTIONS:
    -p, --provider <ID>       Active provider instance (anthropic, openai, gemini, ollama)
    -m, --model <MODEL>       Target model identifier (e.g. claude-3-7-sonnet, o3-mini)
    -r, --runtime <RUNTIME>   Agent runtime (internal-codex, opencode, claude-code)
    -s, --session <ID>        Resume session by ID or 'last'
    -w, --worktree <PATH>     Execute within isolated Git worktree
        --lang <LOCALE>       UI display language [default: system] [possible: en-US, vi-VN]
        --theme <THEME>       Color theme [default: dark] [possible: dark, light, adaptive]
    -y, --auto-approve <LVL>  Approval level [default: readonly] [possible: none, readonly, workspace, all]
        --open-ide            Open active session in Agent Studios Code-OSS IDE
    -h, --help                Print help information
    -V, --version             Print version information

SUBCOMMANDS:
    run        Execute prompt headlessly and output result
    resume     Resume an existing session in interactive TUI
    sessions   List recent sessions and statuses
    model      Inspect or switch models
    runtime    Manage external agent runtimes
    worktree   Manage isolated Git worktrees
    open-ide   Open active session in Code-OSS IDE
```

#### Vietnamese (`vi-VN`)
```text
Agent Studios CLI (`ags`) 0.12.0
Môi trường phát triển đa tác nhân tất định dành cho lập trình viên

CÁCH DÙNG:
    ags [TÙY_CHỌN] [YÊU_CẦU]
    ags <LỆNH_CON>

TÙY_CHỌN:
    -p, --provider <ID>       Nhà cung cấp mô hình (anthropic, openai, gemini, ollama)
    -m, --model <MÔ_HÌNH>     Mô hình ngôn ngữ đích (ví dụ: claude-3-7-sonnet, o3-mini)
    -r, --runtime <RUNTIME>   Môi trường thực thi tác nhân (internal-codex, opencode, claude-code)
    -s, --session <MÃ_PHIÊN>  Khôi phục phiên làm việc theo mã hoặc 'last'
    -w, --worktree <ĐƯỜNG_DẪN> Thực thi trong cây làm việc Git độc lập
        --lang <NGÔN_NGỮ>     Ngôn ngữ giao diện [mặc định: hệ thống] [hỗ trợ: en-US, vi-VN]
        --theme <GIAO_DIỆN>   Giao diện màu sắc [mặc định: dark] [hỗ trợ: dark, light, adaptive]
    -y, --auto-approve <MỨC>  Mức tự động phê duyệt [mặc định: readonly] [hỗ trợ: none, readonly, workspace, all]
        --open-ide            Mở phiên làm việc trong Agent Studios Code-OSS IDE
    -h, --help                In thông tin trợ giúp
    -V, --version             In thông tin phiên bản

LỆNH CON:
    run        Thực thi yêu cầu ở chế độ nền và xuất kết quả
    resume     Tiếp tục phiên làm việc trong giao diện TUI tương tác
    sessions   Danh sách các phiên làm việc gần đây
    model      Kiểm tra hoặc chuyển đổi mô hình
    runtime    Quản lý các môi trường thực thi tác nhân ngoài
    worktree   Quản lý các cây làm việc Git độc lập
    open-ide   Mở phiên làm việc hiện tại trong Code-OSS IDE
```

---

## 4. Invariant: Display Language $\neq$ Agent Response Language

Per Product Principle #26 and `LOCALIZATION.md`:
- Running `ags --lang vi-VN` translates all application borders, status indicators, error codes, and help strings into Vietnamese.
- It **does not** automatically force the AI agent to write code comments or technical explanations in Vietnamese unless the developer's prompt or project `AGENTS.md` explicitly requests it.
- Conversely, a developer running in English mode (`ags --lang en-US`) can prompt in Vietnamese, and the Coordinator will respond naturally in Vietnamese while maintaining English CLI framing.
