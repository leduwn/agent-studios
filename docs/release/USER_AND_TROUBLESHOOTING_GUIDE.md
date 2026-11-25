# Agent Studios — User & Troubleshooting Guide (English / Tiếng Việt)

## 1. Quick Start (English)

1. **Install / Stage**: Run `installer/windows/Install-AgentStudios.ps1` (or stage with `scripts/packaging/Build-WindowsReleaseBundle.ps1`).
2. **Verify Environment**:
   ```powershell
   ags doctor
   ags doctor --json
   ```
3. **Configure a Provider (BYOK)**:
   ```powershell
   ags provider add openai --name "OpenAI" --endpoint "https://api.openai.com/v1" --secret-ref OPENAI_API_KEY --default
   ags provider test <INSTANCE_ID>
   ags provider discover <INSTANCE_ID> --save
   ```
4. **Run a Goal Headlessly**:
   ```powershell
   ags run --headless --demo "Analyze repository structure"
   echo "Refactor helper" | ags run --stdin --headless --demo
   ```
5. **Canonical Windows Data Paths**:
   - Config: `%APPDATA%\AgentStudios\config.toml`
   - SQLite WAL State: `%LOCALAPPDATA%\AgentStudios\state.db`
   - Artifacts: `%LOCALAPPDATA%\AgentStudios\artifacts\`

---

## 2. Hướng dẫn nhanh & Xử lý sự cố (Tiếng Việt)

1. **Kiểm tra môi trường**:
   Chạy `ags doctor` để kiểm tra Git, kích thước terminal, hỗ trợ Unicode NFC tiếng Việt, Control Plane và kết nối tiến trình nền `agsd`.
2. **Chế độ Tác tử (Agent Mode) và Chế độ IDE (IDE Mode)**:
   Chuyển đổi giữa `agent-studios.openInIde` và `agent-studios.switchToAgentMode` giữ nguyên `session_id`, tiến trình `agsd`, danh sách tác vụ và hàng đợi phê duyệt.
3. **Tách biệt ngôn ngữ giao diện và ngôn ngữ phản hồi**:
   Thiết lập `agentStudios.displayLanguage` (`en-US` hoặc `vi-VN`) chỉ đổi ngôn ngữ giao diện. Ngôn ngữ phản hồi của tác tử được kiểm soát độc lập qua `agentStudios.agentResponseLanguage` (`auto`, `en-US`, `vi-VN`).
4. **Khắc phục lỗi thường gặp**:
   - **Daemon offline (`agsd` chưa chạy)**: `ags` tự động chuyển sang `EmbeddedClient` cho tác vụ cục bộ hoặc kiểm tra bằng `ags daemon status --json`.
   - **Approval blocked (mã thoát `2`)**: Trong chế độ `--headless`, tác vụ yêu cầu quyền cao sẽ fail-closed nếu thiếu `--auto-approve medium|high|all`.
   - **Xung đột nhánh công việc (`Blocked`)**: Tác vụ xung đột bản vá được giữ ở trạng thái `Blocked` (không tự ý xóa worktree hay chạy `git reset --hard`) để người dùng kiểm tra trong tab Review.
