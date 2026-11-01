# M15 UI-03 — Cross-Platform IPC & CI Verification

## Metadata
- **Milestone**: M15 UI-03
- **Topic**: Cross-Platform Transport Validation (Windows Named Pipes vs POSIX UDS) & GitHub Actions CI Matrix
- **Pinned Backend Commit**: `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b`
- **Date**: 2026-11-02

---

## 1. IPC Transport Parity

The Agent Studios Extension Host communicates with the compiled Rust daemon (`agsd`) using native OS inter-process communication:

1. **Windows Platform**:
   - Transport: Named Pipes (`\\.\pipe\agent-studios-<key>`).
   - Node.js module: `net.connect(pipeName)`.
   - Framing: 4-byte big-endian length prefix.
   - Verified on Windows 11 Home Single Language x64.

2. **Linux & macOS (POSIX)**:
   - Transport: Unix Domain Sockets (`/tmp/agent-studios-<key>.sock`).
   - Node.js module: `net.connect(socketPath)`.
   - Framing: 4-byte big-endian length prefix.
   - Tested in GitHub Actions `ubuntu-latest`.

---

## 2. GitHub Actions CI Configuration (`.github/workflows/agent-studios-m14-ui-ci.yml`)

The CI workflow matrix exercises:
- `ubuntu-latest` and `windows-latest`.
- Strict checkout and provenance verification of backend commit `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b`:
  ```bash
  PINNED_SHA="cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b"
  git fetch --depth=1 --force origin "$PINNED_SHA":backend-pin
  ACTUAL_SHA=$(git rev-parse backend-pin)
  if [ "$ACTUAL_SHA" != "$PINNED_SHA" ]; then
    echo "::error::Backend commit mismatch: expected $PINNED_SHA but got $ACTUAL_SHA"
    exit 1
  fi
  ```
- Fresh compilation of native `agsd` daemon binary on both Ubuntu and Windows.
- Execution of unit test suite (`npm run test:unit`), real daemon E2E suite (`npm run test:e2e`), and Extension Host integration suite with Xvfb (`npm run test:integration`).
