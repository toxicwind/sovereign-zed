
# sovereign-zed

[![repo](https://img.shields.io/badge/toxicwind-sovereign--zed-blue?logo=github)](https://github.com/toxicwind/sovereign-zed)
[![license](https://img.shields.io/badge/license-GPL--3.0--or--later-green)](./LICENSE-GPL)
![Rust](https://img.shields.io/badge/rust-toolchain-orange?logo=rust)

> **Zed, hardened for the sovereign agent fleet.** A fork of
> [zed-industries/zed](https://github.com/zed-industries/zed) with
> agent-reliability patches, hardened OpenAI-compatible provider plumbing for
> untrusted and free model routers, and a faster release toolchain.

## Sync state

| Item | Value |
|------|-------|
| Upstream | [zed-industries/zed](https://github.com/zed-industries/zed) |
| Upstream commits included through | 2026-08-15 (e.g. PR #62602, #62685, #62284) |
| Last upstream rebase merge | 2026-09-03 (`a408668` — integrate origin/main with rebased upstream and providers) |
| Last fork commit | 2026-09-07 (`f0b2e9c` — shell_env.rs type annotations and borrow fixes) |

Patches are rebased onto latest `origin/main` with [`sync-upstream.sh`](./sync-upstream.sh).

## What is different from upstream

Every patch below is verified in this repo git history (commit hashes on `main`).

| Area | Patch | Why | Commit | Files |
|------|-------|-----|--------|-------|
| Agent | **Gemini `const` schema sanitizer** | Google Gemini API rejects `const` in `function_declarations.parameters`. Strips `const`, collapses `anyOf`-with-const → `enum`, drops `if/then/else` before constructing `FunctionDeclaration`. | `14c3042` | `crates/google_ai/src/completion.rs` |
| Agent | **Tool arg normalizer** | Models emit OpenAI/Anthropic field names (`working_directory`→`cd`, `file_path`→`path`, `query`→`regex`, `content`→`edits`). Maps aliases per tool, coerces string→u64 for `timeout_ms`. | `14c3042` | `crates/agent/src/tools.rs`, `crates/agent/src/thread.rs` |
| Agent | **Terminal `cd` default** | Models that omit `cd` hit deserialization error. `#[serde(default)]` so `.` is used when absent. | `14c3042` | `crates/agent/src/tools/terminal_tool.rs` |
| Agent | **`autonomous_edits` capability flag** | Least-privilege at the tool-dispatch boundary: per-model flag that gates whether a model may change things outside the worktree autonomously. See [AUDIT.md](./AUDIT.md). | `787d707` | `crates/language_model/`, `crates/settings_content/`, `crates/agent/src/tool_permissions.rs` |
| Providers | **Hardened OpenAI-compatible providers** | `openai_mcpproxy` (full JSON Schema, self-healing tool-call mapper), NVIDIA/Inkling variant, Vercel AI gateway, `interleaved_reasoning` round-trip. Built for untrusted/free model routers (mcpproxy, NVIDIA NIM, sovereign-router). | `7637bdc` | `crates/language_models/provider/` |
| Search | **Grep respects `.ignore`** | Multi-GB JSONL/trajectory dirs OOM upstream grep. Patterns from `.ignore` merged into exclusion matcher. | `cd190e2` | `crates/agent/src/tools/grep_tool.rs` |
| Search | **ast-grep dev helpers** | Shell wrapper for regex+structural search; `--help` to JSON schema parser. | — | `scripts/` |
| Build | **sccache + mold** | Default `rustc-wrapper = sccache`, links with `-fuse-ld=mold`. Cached build script for fast release rebuilds. | `82699e6` | `.cargo/config.toml`, `script/build-release-cached` |
| Sync | **`sync-upstream.sh`** | Rebases patches onto latest `origin/main`. | — | `sync-upstream.sh` |
| Ext | **GHAS external-access extension** | Pre-configured context-server profiles (local, Tailscale, external) for reaching the GHAS MCP server from mobile devices. Upstream Zed has no built-in external MCP access. | `a3ac99f` | `extensions/ghas-external-access/` |

### What is *not* in this fork

Language model endpoints, provider configs (OpenRouter, NVIDIA NIM, Google,
Mistral, etc.), and API URLs live in **user settings**
(`~/.config/zed/settings.json`) and deploy scripts — not in this git delta.

See [AUDIT.md](./AUDIT.md) for the full repo map of the load-bearing crates
(`crates/language_model/`, `crates/settings_content/`,
`crates/language_models/provider/`, `crates/agent/`).

## GHAS external access

| Component | Purpose | Files |
|-----------|---------|-------|
| **`ghas-external-access`** | External access configuration for GHAS MCP server | `extensions/ghas-external-access/` |
| **Context server configs** | Pre-configured remote access profiles (local, Tailscale, external) | `extensions/ghas-external-access/extension.toml` |
| **Documentation** | Setup guides for secure remote access | `extensions/ghas-external-access/README.md` |

Usage:
1. Ensure GHAS MCP is running: `pgrep -f ghas-mcp-stdio.sh`
2. Add context server config to `~/.config/zed/settings.json`
3. Connect via Tailscale or configure firewall rules

**Security note:** never expose MCP servers directly to the internet. Use
Tailscale, WireGuard, or similar zero-trust networking.

## Build

```bash
# from your checkout of this repo
cargo build --release
```

Uses the **sccache build server** for distributed compilation caching and the
**mold linker** for fast linking. Configured in `.cargo/config.toml`. Also
available via the **forgewatch** build queue server
(`http://127.0.0.1:7878`): `forgewatch run --file forgewatch.yml`.

Binary installs to `~/.local/bin/zed`; desktop entry at
`~/.local/share/applications/dev.zed.Zed.desktop`.

## Development

- [Building Zed for Linux](./docs/src/development/linux.md)
- [Building Zed for macOS](./docs/src/development/macos.md)
- [Building Zed for Windows](./docs/src/development/windows.md)

See [CONTRIBUTING.md](./CONTRIBUTING.md) for ways to contribute.

## Remotes

```text
upstream  https://github.com/zed-industries/zed.git   (read-only)
origin    https://github.com/toxicwind/sovereign-zed.git  (this repo)
```

## Security

- `crates/agent/src/tool_permissions.rs` — central permission gate
  (`ToolPermissionDecision`: Allow/Deny/Confirm); hardcoded rules deny
  `rm -rf /`, `~`, `..`, `$HOME` in terminal tools.
- The `autonomous_edits` per-model flag enforces least-privilege at the
  tool-dispatch boundary.
- Never commit provider API keys; they belong in user settings, not in this
  delta.

## License

Zed source code is licensed primarily under **GPL-3.0-or-later**, with
Apache-2.0 components where marked — see [LICENSE-GPL](./LICENSE-GPL) and
[LICENSE-APACHE](./LICENSE-APACHE). Third-party dependency license info must
be correct for CI to pass (see `script/licenses/zed-licenses.toml`).
