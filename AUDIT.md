# AUDIT — toxicwind/zed (Zed fork)

> Audited: 2026-07-20. Goal: understand WHY the agent behaves the way it does
> (scope refusals, endless tool-call retry loops) at the code level, so fixes
> are root-cause rather than superstition.

## Verdict

A thin, well-structured fork of `zed-industries/zed` focused on **agent
reliability** (provider hardening, tool-call normalization, terminal `cd`
default) and **build toolchain** (sccache + mold). The three custom providers
(`nvidia`, `openai_mcpproxy`, `openai_mcpproxy_nvidia`) live in
`crates/language_models/src/provider/`. The fork is sound; the "weird" agent
behaviors the user hits are **upstream Zed design choices**, not fork bugs.

## Repo map

| Path | Role |
|------|------|
| `crates/agent/src/thread.rs` | The agent turn loop, completion retry logic, tool-result handling. |
| `crates/agent/src/tools/tool_permissions.rs` | `resolve_project_path` — the scope enforcement. |
| `crates/agent/src/tools/read_file_tool.rs` | Read path resolution; special allow for `~/.agents/skills`. |
| `crates/agent/src/tools/edit_file_tool.rs` | Edit path resolution (same scope machinery). |
| `crates/agent/src/tools/terminal_tool.rs` | Shell; its own `cd` root check (separate from file tools). |
| `crates/language_models/src/provider/*.rs` | Provider implementations incl. the 3 custom ones. |
| `crates/nvidia/` | `Model` enum + `NVIDIA_API_URL` constant. |
| `crates/settings_content/src/language_model.rs` | Settings schema for providers. |

## WHY #1 — "file tool refused the path" (scope boundary)

- Enforcement: `tool_permissions::resolve_project_path` → `project.find_project_path`
  → bails `"Path {p} is not in the project"`.
- `read_file`/`edit_file`/`write_file` resolve through this. They are **scoped to
  the project's worktree roots by design** — a sandbox/security boundary, not a
  bug.
- `read_file` has ONE deliberate escape: `resolve_global_skill_path` allows
  `~/.agents/skills` (and descendants) so skill files are editable. That is the
  only out-of-scope path the file tools accept.
- `terminal` does NOT use this machinery for its *commands* — only its `cd`
  argument is checked (`terminal_tool.rs:1274`: "`cd` directory was not in any
  root directory"). So `terminal` can read/edit ANY path the OS user can reach
  (e.g. `~/.config/zed/settings.json`, a fork outside the workspace).
- **Implication for the agent:** when a file tool refuses an out-of-scope path,
  the correct move is NOT to retry it — it is to use `terminal` (cat/sed/python3)
  or, for skill files, the file tools directly. The refusal is expected friction,
  not an error to fight.

## WHY #2 — the endless tool-call retry loop

- The turn loop is in `thread.rs`. On a failed completion it calls
  `retry_completion_error` → `handle_completion_error` → `retry_strategy_for`.
- `retry_strategy_for` (thread.rs:4409) classifies errors:
  - Auth / payload-too-large / endpoint-not-found / payment → `None` (no retry).
  - Rate limit / 503 / 529 → retry up to `MAX_RETRY_ATTEMPTS` (4).
  - **Any 4xx/5xx `HttpResponseError` → retry up to 3** (line ~4470).
  - `UpstreamProviderError` 500 → retry up to 3 (line ~4443).
- `MAX_RETRY_ATTEMPTS = 4`, `BASE_RETRY_DELAY = 5s` (thread.rs:166-167).
- **Root cause of the loop:** NVIDIA's Outlines backend returns **HTTP 500**
  ("Could not translate instance to regex") when it receives a collapsed JSON
  Schema (`JsonSchemaSubset`) on a large tool set. Zed classifies 500 as a
  *transient* upstream error and retries. The retried request is **byte-identical**,
  so it 500s again. Each new agent turn re-issues the same bad request → the loop
  appears endless (3 retries/turn × repeated turns).
- **The fix (already in this fork):** the custom providers send **full JSON
  Schema** (`tool_input_format: JsonSchema`) + a **non-destructive schema
  normalizer** + `interleaved_reasoning: true` + `prompt_cache_key: false`. This
  makes the request *not* 500 in the first place. Additionally, the
  `openai_mcpproxy` providers add a **self-healing event mapper** that converts a
  `ToolUseJsonParseError` into a valid `ToolUse` with `{}` input, so a malformed
  streamed tool call terminates the turn instead of retrying forever.
- **Note:** `ToolUseJsonParseError` itself is NOT retried at the completion level
  (it's a streamed event, handled in `process_tool_result`); the danger was the
  *provider* 500, not the parse error. The self-heal is belt-and-suspenders.

## Security model

- Path scope is the primary trust boundary (see WHY #1). Symlink escapes are
  detected (`ResolvedProjectPath::SymlinkEscape`) and would prompt the user.
- `file_scan_exclusions` / `private_files` settings can block reads even inside
  the project (read_file_tool.rs:295+).

## Build / run

```bash
cd /home/toxic/projects/zed
source "$HOME/.cargo/env"   # sccache + mold configured in .cargo/config.toml
cargo check -p language_models
cargo test -p language_models provider
./script/build-release-cached
```

## Gotchas

- `read_file`/`edit_file`/`write_file` refuse paths outside the project root(s)
  except `~/.agents/skills`. Use `terminal` for everything else.
- `terminal`'s `cd` param is also root-scoped; an inline `cd` inside the command
  works anywhere. Multi-line `&&` + var assignments can fail in multi-root
  workspaces — write a script file instead.
- A provider 500 is treated as retryable by upstream Zed; the only durable fix is
  to stop sending the request shape that 500s (full JSON Schema, no
  `prompt_cache_key`).
