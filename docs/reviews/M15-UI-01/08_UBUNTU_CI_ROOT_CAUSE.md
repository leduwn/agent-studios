# Milestone M15 UI-01.1 Review: Ubuntu CI Root Cause Analysis

**Run ID**: `41099139076`  
**Platform**: Ubuntu POSIX (`ubuntu-latest`)  
**Failing Test Suite**: `OperatorConfirmationModal.test.ts`  
**Status**: RESOLVED  

---

## 1. Root Cause Analysis

In CI workflow run `41099139076`, the Ubuntu Linux runner failed with the following error:

```text
Error: Foreign-platform Windows path 'D:\Projects\agent-studios-ide-ui' cannot be safely interpreted on POSIX platform.
    at normalizeWorkspacePath (src/bridge/ipc/CapabilityMatrix.ts)
    at OperatorConfirmationModal.requestConfirmation (src/security/OperatorConfirmationModal.ts)
    at Context.<anonymous> (src/test/unit/OperatorConfirmationModal.test.ts:45:34)
```

### Mechanism of Failure
1. Under Milestone M14-UI-00, `normalizeWorkspacePath` was hardened with strict cross-platform security guards:
   - On Windows, POSIX paths without drive letters are normalized or evaluated according to Windows semantics.
   - On POSIX (Linux/macOS), Windows drive-letter paths (e.g. `D:\...`, `C:\...`) and UNC paths (`\\server\share`) are strictly rejected with an explicit error because attempting to resolve them using POSIX `path.resolve` or `fs.realpathSync` treats drive letters as relative directories (`./D:`) or causes silent path escapes.
2. In `OperatorConfirmationModal.test.ts`, the test suite previously contained a hardcoded Windows fixture path:
   ```typescript
   // Defective fixture:
   const workspacePath = 'D:\\Projects\\agent-studios-ide-ui';
   ```
3. When `OperatorConfirmationModal.requestConfirmation` was invoked with this path on Linux, `normalizeWorkspacePath` correctly triggered its fail-closed protection and rejected the foreign-platform path.

---

## 2. Implemented Correction

The hardcoded Windows path fixture was replaced with platform-agnostic, OS-native temporary directories created during test setup:

```typescript
let tempDir: string;
let testWorkspace: string;

beforeEach(() => {
  OperatorConfirmationManager.resetForTesting();
  OperatorConfirmationModal.resetDialogHandlerForTesting();
  tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ag-modal-test-'));
  testWorkspace = fs.realpathSync(tempDir);
});

afterEach(() => {
  try {
    fs.rmSync(tempDir, { recursive: true, force: true });
  } catch {
    // Ignore cleanup errors
  }
});
```

### Verification
- On POSIX, `os.tmpdir()` creates `/tmp/ag-modal-test-XXXXXX`, which passes POSIX path validation, symlink canonicalization, and workspace containment checks.
- On Windows, `os.tmpdir()` creates `C:\Users\...\AppData\Local\Temp\ag-modal-test-XXXXXX`, canonicalized with drive-letter lowercasing and normalized slashes.
- Cross-platform test cases were added to explicitly verify that foreign-platform paths are safely rejected while native paths pass all security gates.
