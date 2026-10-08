# Agent Studios — Localization & Internationalization Architecture

> **Status**: Core Architecture Specification
> **Precedence**: Subservient to `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`.

---

## 1. First-Class English & Vietnamese Support

Localization is a core architectural requirement in Agent Studios, not a superficial cosmetic skin:

- **Day-One Parity**: English (`en-US`) and Vietnamese (`vi-VN`) are first-class, co-equal UI languages across all desktop views, setup wizards, settings dialogs, and CLI diagnostics.
- **Natural Terminology**: Vietnamese translations use professional, native developer terminology (e.g., *Nhánh*, *Bản vá*, *Cây tác vụ*, *Không gian làm việc*) rather than clumsy machine-translated jargon.
- **Zero Hardcoded Strings**: All user-facing UI labels, error messages, placeholders, and tooltips are externalized into structured localization bundles (`en.json`, `vi.json`).

---

## 2. Invariant: Display Language $\neq$ Agent Response Language

Agent Studios strictly decouples the application UI display language from the LLM conversation language:

$$\textbf{UI Display Language} \quad\not\equiv\quad \textbf{Agent Response Language}$$

- A developer can operate the Agent Studios IDE with a **Vietnamese UI** while instructing agents to converse, document, and comment code in **English**.
- Conversely, a developer using an **English UI** can converse with the Coordinator in **Vietnamese**.
- The cognitive Coordinator automatically mirrors the user's conversational language unless explicitly instructed otherwise (e.g., "Answer in English" or "Viết tài liệu bằng tiếng Việt").
- System prompts, tool schemas, and Git commit messages maintain their standard technical English conventions unless project guidelines (`AGENTS.md`) specify otherwise.

---

## 3. Localization Fallback Hierarchy

When resolving localized strings across regional variations or incomplete bundles, the engine follows a deterministic fallback chain:

```text
User Selected Locale (e.g., vi-VN)
             │
             ▼ (If key missing)
Base Language Bundle (vi)
             │
             ▼ (If key missing)
Canonical Default: English (en-US)
             │
             ▼ (If key missing)
Raw Key Identifier (`[missing: tasks.graph.reconciliation]`)
```

This guarantees that an incomplete translation bundle never crashes the desktop shell or renders blank UI elements.

---

## 4. Internationalization & RTL Readiness

1. **Unicode Everywhere**: UTF-8 encoding is enforced end-to-end across file reading, patch creation, string hashing, and event serialization.
2. **Pluralization & Formatting**: Uses standard ICU message format for dynamic pluralization and date/currency formatting across locales.
3. **Right-to-Left (RTL) Readiness**: UI layout engines use CSS logical properties (`margin-inline-start`, `inset-inline-end`, `text-align: start`) to ensure seamless future support for RTL languages (Arabic, Hebrew) without redesigning layout containers.
