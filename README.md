# Zed (qed fork)

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

Welcome to **qed**, a high-performance, multiplayer code editor fork of [Zed](https://zed.dev) with sovereign agent support, LLM routing, and optimized build toolchain.

---

## 🔱 toxicwind/qed

*Last synced with upstream: **Jul 20, 2026** — [diff](https://github.com/toxicwind/qed/compare/main:zed-industries/zed)*

This is a **thin fork** of [zed-industries/zed](https://github.com/zed-industries/zed) with patches focused on agent reliability, code search, and build toolchain. Every patch is meant for upstream — they just haven't gotten there yet.

### What's different

| | Area | Patch | Why | Files |
|-|------|-------|-----|-------|
| 🐛 | **Agent** | **Gemini `const` schema sanitizer** | Google's Gemini API rejects `const` in `function_declarations.parameters`. Strips `const`, collapses `anyOf`-with-const → `enum`, drops `if/then/else` before constructing `FunctionDeclaration`. | `crates/google_ai/src/completion.rs` |
| 🐛 | **Agent** | **Tool arg normalizer** | Models emit OpenAI/Anthropic field names (`working_directory`→`cd`, `file_path`→`path`, `query`→`regex`, `content`→`edits`). Maps aliases per tool, coerces string→u64 for `timeout_ms`. Eliminates `thread.rs:1635` validation errors. | `crates/agent/src/tools.rs`, `crates/agent/src/thread.rs` |
| 🐛 | **Agent** | **Terminal `cd` default** | Models that omit `cd` hit deserialization error. `#[serde(default)]` so `.` is used when absent. | `crates/agent/src/tools/terminal_tool.rs` |
| 🔍 | **Search** | **Grep respects `.ignore`** | Multi-GB JSONL/trajectory dirs OOM'd upstream grep. Patterns from `.ignore` merged into exclusion matcher. | `crates/agent/src/tools/grep_tool.rs` |
| 🔍 | **Search** | **ast-grep dev helpers** | Shell wrapper for regex+structural search; `--help` to JSON schema parser. | `scripts/` |
| ⚙️ | **Build** | **sccache + mold** | Default `rustc-wrapper = sccache`, links with `-fuse-ld=mold`. Cached build script for fast release rebuilds. | `.cargo/config.toml`, `script/build-release-cached` |
| 🔄 | **Sync** | **`sync-upstream.sh`** | Rebases patches onto latest `origin/main` and force-pushes `fork`. | `sync-upstream.sh` |
| 🤖 | **Router** | **pi-auto-router** | Multi-provider LLM routing with budget-aware policies and same-request failover. | `~/.tau/agent/extensions/pi-auto-router/` |

### What's *not* in this fork

Language model endpoints, provider configs (OpenRouter, NVIDIA NIM, Google, Mistral, etc.), and API URLs all live in **user settings** (`~/.config/zed/settings.json`) and deploy scripts — not in this git delta. Inference runs on **[llama-swap](https://github.com/toxicwind/llama-swap-main)** (`:25100`). No vLLM.

### What's *new* in this fork

#### 🔗 External Access — GHAS MCP Remote Configuration

| Component | Purpose | Files |
|-----------|---------|-------|
| **`ghas-external-access`** | External access configuration for GHAS MCP server | `extensions/ghas-external-access/` |
| **Context Server Configs** | Pre-configured remote access profiles (local, Tailscale, external) | `extensions/ghas-external-access/extension.toml` |
| **Documentation** | Setup guides for secure remote access | `extensions/ghas-external-access/README.md` |

**Why it exists:** The upstream Zed editor lacks built-in support for external MCP server access from mobile devices. This extension provides pre-configured context server profiles that integrate with:
- **Tailscale** for secure VPN-based access
- **Local development** for testing
- **External IP** configurations for controlled remote access

**Usage:**
1. Ensure GHAS MCP is running: `pgrep -f ghas-mcp-stdio.sh`
2. Add context server config to `~/.config/zed/settings.json`
3. Connect via Tailscale or configure firewall rules
4. Access Zed agent with full GHAS capabilities from any device

**Security note:** Never expose MCP servers directly to the internet. Use Tailscale, WireGuard, or similar zero-trust networking.

#### 🤖 pi-auto-router Extension

| Component | Purpose | Files |
|-----------|---------|-------|
| **pi-auto-router** | Multi-provider LLM routing with budget-aware policies | `~/.tau/agent/extensions/pi-auto-router/` |
| **Tau coding agent** | Agent runtime with web parts, reasoning, tool use | `~/.tau/` |
| **llama-swap** | Local inference server at `:25100` | `~/projects/llama-swap-main/` |

**Why it exists:** The upstream Zed editor has no built-in LLM router. pi-auto-router provides automatic provider selection, budget management, and failover across OpenRouter, NVIDIA NIM, Google, and local models.

**Usage:**
1. Install: `bun install --production` in `~/.tau/agent/extensions/pi-auto-router/`
2. Configure providers in `~/.config/tau/settings.json`
3. Web parts are max-enabled for full agent capabilities
4. Launch tau in tmux: `tmux new-session -d -s tau 'tau'`

### Build

```bash
cd /home/toxic/projects/qed/editor
cargo build --release
```

Uses the **sccache build server** (daemon PID 2063211) for distributed compilation caching and **mold linker** for fast linking. Configured in `.cargo/config.toml`.

Also available via **forgewatch** build queue server at `http://127.0.0.1:7878`:
```bash
forgewatch run --file forgewatch.yml
```

### Remotes

```text
origin  https://github.com/zed-industries/zed.git   (upstream, read-only)
fork    https://github.com/toxicwind/qed.git         (this repo)
```

### Install

```bash
# Binary is at /home/toxic/.local/bin/zed (symlink to zed-dev → target/release/zed)
# Desktop entry: ~/.local/share/applications/dev.zed.Zed.desktop
# Icon: ~/.local/share/icons/hicolor/256x256/apps/zed-sovereign.png
```

### Development

- [Building Zed for Linux](./docs/src/development/linux.md)
- [Building Zed for macOS](./docs/src/development/macos.md)
- [Building Zed for Windows](./docs/src/development/windows.md)

### Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for ways you can contribute to Zed.

Also... we're hiring! Check out our [jobs](https://zed.dev/jobs) page for open roles.

### Licensing

Zed source code is licensed primarily under GPL-3.0-or-later, with Apache-2.0 components where marked.

License information for third party dependencies must be correctly provided for CI to pass.

We use [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) to automatically comply with open source licenses. If CI is failing, check the following:

- Is it showing a `no license specified` error for a crate you've created? If so, add `publish = false` under `[package]` in your crate's Cargo.toml.
- Is the error `failed to satisfy license requirements` for a dependency? If so, first determine what license the project has and whether this system is sufficient to comply with this license's requirements. If you're unsure, ask a lawyer. Once you've verified that this system is acceptable add the license's SPDX identifier to the `accepted` array in `script/licenses/zed-licenses.toml`.
- Is `cargo-about` unable to find the license for a dependency? If so, add a clarification field at the end of `script/licenses/zed-licenses.toml`, as specified in the [cargo-about book](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).

## Sponsorship

Zed is developed by **Zed Industries, Inc.**, a for-profit company.

If you'd like to financially support the project, you can do so via GitHub Sponsors.
Sponsorships go directly to Zed Industries and are used as general company revenue.
There are no perks or entitlements associated with sponsorship.
