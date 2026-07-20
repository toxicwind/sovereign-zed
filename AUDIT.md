# AUDIT.md — Zed Fork (`/home/toxic/projects/zed`)

**Verdict:** A Rust/GPUI fork of Zed (the Zed Industries editor + agent). This
fork adds hardened OpenAI-compatible provider plumbing for untrusted/free model
routers (mcpproxy, NVIDIA NIM, sovereign-router) and — as of this audit — a
per-model **`autonomous_edits`** capability flag that enforces least-privilege at
the tool-dispatch boundary. The feature is the implementable version of "don't
let this model change things outside the worktree autonomously."

## Repo map (load-bearing crates)

- `crates/language_model/` — the `LanguageModel` trait (capability accessors:
  `supports_tools`, `supports_images`, **`supports_autonomous_edits`**).
- `crates/settings_content/` — `OpenAiCompatibleModelCapabilities` struct
  (the per-model capability flags: `tools`, `images`, `parallel_tool_calls`,
  `prompt_cache_key`, `chat_completions`, `interleaved_reasoning`,
  `max_tokens_parameter`, **`autonomous_edits`**).
- `crates/language_models/` — provider impls:
  - `provider/open_ai_compatible.rs` — generic OpenAI-compatible.
  - `provider/openai_mcpproxy.rs` — mcpproxy-hardened (full JSON Schema,
    self-healing tool-call mapper).
  - `provider/openai_mcpproxy_nvidia.rs` — NVIDIA/Inkling variant.
  - `provider/vercel_ai_gateway.rs` — Vercel AI gateway.
- `crates/agent/` — the agent runtime:
  - `src/thread.rs` — `Thread` + `ToolCallEventStream`; `authorize()` is the
    central permission gate.
  - `src/tool_permissions.rs` — `ToolPermissionDecision` (Allow/Deny/Confirm),
    `HARDCODED_SECURITY_RULES` (denies `rm -rf /`, `~`, `..`, `$HOME` in
    terminal), `decide_permission_from_settings`.
  - `src/tools/` — individual tool definitions, each with a `NAME` const.

## Architecture / data flow

1. A model is selected per-thread (`ThreadModel`).
2. When a tool fires, `Thread::authorize()` builds a `ToolPermissionContext`
   and a `check_settings` closure.
3. The closure calls `decide_permission_from_settings` →
   `ToolPermissionDecision::from_input` (hardcoded rules + user `tool_permissions`).
4. **New gate (this fork):** before consulting settings, the closure checks
   `is_mutating_tool(tool_name) && !model.supports_autonomous_edits()`. If true,
   it returns `Deny` — a hard boundary that user settings cannot override.

## The `autonomous_edits` feature (why it exists)

**Systems / game-theory rationale:** In a sudo-capable agentic system, the
principal–agent problem means an untrusted (free) model has incentive to expand
its action space (scope creep, reward hacking) because the cost of overreach is
borne by the user. A prompt asking the model to "behave" is a *soft* norm; a
capability flag enforced at tool dispatch is a *credible commitment device* —
the only kind of constraint that is game-theoretically binding. It is capability-
based access control (CBAC) per model, fail-safe default `true`.

**Wiring:**
- `OpenAiCompatibleModelCapabilities.autonomous_edits: bool` (default `true`),
  serde `#[serde(default = "default_true")]`.
- `LanguageModel::supports_autonomous_edits()` default method (returns `true`
  for non-OpenAI-compatible providers).
- Provider impls (`open_ai_compatible`, `openai_mcpproxy`, `openai_mcpproxy_nvidia`)
  return `self.model.capabilities.autonomous_edits`.
- `Thread::authorize` (in `ToolCallEventStream`) denies mutating tools
  (`write_file`, `edit_file`, `delete_path`, `move_path`, `rename_symbol`,
  `create_directory`, `copy_path`, `terminal`, `apply_code_action`) when the
  model's flag is `false`.

## Security model

- Hardcoded security rules are non-bypassable (terminal only).
- `tool_permissions` settings provide user-level Allow/Deny/Confirm per tool.
- `autonomous_edits` is a *model-level* hard override layered *above* settings:
  a model without it can never mutate, regardless of `tool_permissions`.
- `session.trust_all_worktrees` (sovereign config) widens trust scope; pair it
  with `autonomous_edits: false` on untrusted models to keep them read-only.

## Build / run / setup

- `cargo check -p settings_content -p language_model -p language_models -p agent`
  — validates the capability + gate changes (all pass).
- `cargo check -p zed` — full binary (needs ~3GB free; disk was full during audit).
- Provider settings live in `~/.config/zed/settings.json` or
  `<project>/.zed/settings.json` under `openai_compatible` / `openai_mcpproxy` /
  `openai_mcpproxy_nvidia` keys.

## Gotchas
- `edit_file`/`write_file` native tools are scoped to project roots; out-of-root
  edits (e.g. `crates/`) must go through `desktop-commander:write_file` or git.
- `OpenAiCompatibleModelCapabilities` literals that omit `..Default::default()`
  must list every field (vercel_ai_gateway.rs needed `autonomous_edits: true`).
- `Thread::authorize` lives on `ToolCallEventStream`, which holds
  `thread: Option<WeakEntity<Thread>>` — the model is reached via
  `thread.upgrade().read(cx).model.as_model()`, NOT `self.model`.
- `WeakEntity` is `Clone` (not `Copy`); capture it by `.clone()` into the
  `'static` permission closure.

## WHY: "no AGENTS.md prevention code"

There is no code that "prevents AGENTS.md from making changes." `AGENTS.md` is
prompt context only. The real autonomy boundary is `tool_permissions.rs` +
the per-model `capabilities` struct + the agent `tools` enable/disable map. The
`autonomous_edits` flag is the correct, implementable cut of the user's intent:
a per-model capability enforced at dispatch.
