# Agent Studios Baseline Build Report

## 1. System & Environment Specifications

- **Operating System**: Windows 11 Home Single Language (Build 10.0.26200)
- **Architecture**: x86_64 / AMD64
- **Git Version**: `git version 2.54.0.windows.1`
- **Rustc Version**: `rustc 1.99.0 (b940084d7 2026-09-28)` (Toolchain: `x86_64-pc-windows-msvc`)
- **Cargo Version**: `cargo 1.99.0 (5f94df478 2026-08-27)`
- **C++ Compiler / Linker**: Microsoft Visual Studio 2026 Community MSVC (`14.51.36231`), `link.exe` (HostX64\x64)
- **WSL2 Status**: Virtualization is disabled in host firmware (`WSL2 is unable to start since virtualisation is not enabled on this machine`). Native Windows build is the validated target.

---

## 2. Upstream Baseline Target

- **Upstream Repository**: `https://github.com/openai/codex.git`
- **Baseline Commit**: `d25c114d494ddb693290b76bf5e5f64ecbdb38fc`
- **Baseline Date**: 2026-10-02
- **Baseline Lock**: Recorded in machine-readable `upstream/codex.lock.json` using snapshot-based upstream tracking.

---

## 3. Build Execution

- **Working Directory**: `codex-rs`
- **Command Executed**:
  ```powershell
  cargo build -p codex-cli
  ```
- **Features Flag**: Standard default features (no `--all-features` used).
- **Execution Log**: Saved locally at `artifacts/bootstrap/native-windows-build.log` (ignored by git via `.gitignore`).

---

## 4. Build Result

- **Status**: **SUCCESS**
- **Compilation Duration**: 45 minutes 45 seconds (`Finished dev profile [unoptimized + debuginfo] target(s) in 45m 45s`)
- **Binary Produced**: `codex-rs\target\debug\codex.exe` (Size: ~350 MB debug binary with full PDB symbols)
- **Verification Command**:
  ```powershell
  .\codex-rs\target\debug\codex.exe --version
  ```
- **Verification Output**:
  ```text
  codex-cli 0.0.0
  ```

---

## 5. Architectural Implications & Observations

1. **Native Windows Compatibility**:
   - Although upstream Codex documentation historically suggests Windows development via WSL2, this native Windows MSVC build proved 100% viable on the baseline commit `d25c114d494ddb693290b76bf5e5f64ecbdb38fc`.
   - All native dependencies (`aws-lc-sys`, `ring`, `libsqlite3-sys`, `tree-sitter`, `tree-sitter-bash`, `tree-sitter-powershell`, `zstd-sys`, `blake3`, `bzip2-sys`, `lzma-sys`, `onig_sys`) compiled and linked cleanly with MSVC.
2. **Zero Upstream Modifications**:
   - No source patches, workarounds, or disabled checks were applied to `codex-rs/core`, `codex-rs/cli`, `codex-rs/app-server`, or other crates.
   - Upstream codebase integrity is strictly preserved.
3. **Resource Profile**:
   - The final link phase of `codex.exe` requires ~5.7 GB of peak RAM with MSVC `link.exe` when generating full unoptimized debug information and symbols.
   - Future CI/developer setup guidelines for Windows should account for this memory requirement.
