# Milestone M15 UI-01.1 Review: Prompt Integrity & Wire Framing Verification

**Target Components**:
- `ChatWebviewViewProvider._handleSubmitTask`
- `ChatSecurityGuards.validateCommandSchema`
- `RustIpcCodec.encodeCommand`

---

## 1. Verbatim Whitespace Preservation

### Defect Identified
Earlier drafts called `message.prompt.trim()`, which stripped leading and trailing newlines, code indentation, and formatted Markdown or Python code blocks.

### Correction
- The destructive `.trim()` call was removed entirely from `_handleSubmitTask`.
- Whitespace-only detection is performed without mutating the prompt string:
  ```typescript
  const prompt = message.prompt || '';
  if (!prompt || /^\s*$/.test(prompt)) {
    await this.postMessage({ type: 'error', error: 'Prompt cannot be empty.' });
    return;
  }
  ```
- The raw prompt is passed verbatim into `OperatorConfirmationModal.requestConfirmation` and `bridge.submitTask`.

---

## 2. Accurate UTF-8 Byte Length Measurement

### Framing Guard Verification
- Wire framing enforces a 4-byte big-endian length prefix and a maximum payload size of 4 MiB (`4 * 1024 * 1024` bytes).
- JavaScript `prompt.length` counts UTF-16 code units, which miscounts multi-byte UTF-8 sequences (emojis, CJK characters, accented characters) and could allow oversized frames to trigger deserializer panics.
- Byte size is measured strictly using Node.js `Buffer.byteLength(prompt, 'utf8')`:
  ```typescript
  const promptByteLength = Buffer.byteLength(prompt, 'utf8');
  if (promptByteLength > 4 * 1024 * 1024) {
    await this.postMessage({ type: 'error', error: 'Prompt exceeds 4 MiB wire frame limit.' });
    return;
  }
  if (promptByteLength > 100 * 1024) {
    void vscode.window.showWarningMessage('Prompt size is larger than 100 KB; wire framing limits may apply.');
  }
  ```

---

## 3. Wire Encoding Verification
Before `IpcCommand::SubmitTask` is transmitted over the native socket or named pipe, `SubmitTaskAdapter` executes `RustIpcCodec.encodeCommand(command)` in preflight. If the encoded wire frame exceeds 4 MiB or fails big-endian length prefixing, it rejects immediately with a `TaskSubmissionError` before any bytes are written to the transport.
