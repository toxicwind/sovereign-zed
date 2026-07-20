# Zed

[![Zed](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/zed-industries/zed/main/assets/badge/v0.json)](https://zed.dev)
[![CI](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml/badge.svg)](https://github.com/zed-industries/zed/actions/workflows/run_tests.yml)

Welcome to Zed, a high-performance, multiplayer code editor from the creators of [Atom](https://github.com/atom/atom) and [Tree-sitter](https://github.com/tree-sitter/tree-sitter).

---

## toxicwind fork ([toxicwind/zed](https://github.com/toxicwind/zed))

Upstream: [zed-industries/zed](https://github.com/zed-industries/zed).  
Private mirror: `toxicwind/zed-source` (`private` remote). Sync helper: `./sync-upstream.sh`.

This tree is a **thin fork**. Everything below is **local-only** relative to `origin/main` (zed-industries). Keep these when rebasing.

### What we added (and why)

| Change | Why | Files |
|--------|-----|--------|
| **Agent grep respects `.ignore`** | Host trees contain multi‑GB JSONL / trajectories. Upstream grep only used gitignore-like defaults and **OOM'd** the agent on sovereign / antigravity dumps. Patterns from each worktree's `.ignore` are merged into the exclusion matcher. | `crates/agent/src/tools/grep_tool.rs` |
| **Tests for `.ignore` + path helpers** | Guard the above; `extract_paths_from_results` reused outside tests. | same |
| **Gemini `const` schema sanitizer** | Zed sends JSON Schema with `const` keyword; Google's Gemini API rejects it with `Unknown name "const"`. Strips `const`, collapses `anyOf`-with-const -> `enum`, drops `if/then/else` before constructing `FunctionDeclaration`. Prevents tool-call failures on all Gemini models. | `crates/google_ai/src/completion.rs` |
| **Tool arg normalizer** | Models emit OpenAI/Anthropic-style field names (`working_directory`, `file_path`, `query`, `content`) but Zed expects `cd`, `path`, `regex`, `edits`. Maps common aliases per tool and coerces string->u64 for `timeout_ms`. Eliminates `thread.rs:1635 missing field` validation errors across terminal, edit_file, write_file, and grep tools. | `crates/agent/src/tools.rs`, `crates/agent/src/thread.rs` |
| **Terminal `cd` default** | Models that omit `cd` from terminal calls hit a deserialization error. Adds `#[serde(default)]` so `.` is used when absent. | `crates/agent/src/tools/terminal_tool.rs` |
| **Release build: sccache + mold** | Full Zed release rebuilds are brutal on this machine. Default `rustc-wrapper = sccache`, x86_64-linux links with **mold**, `script/build-release-cached` for agent/install paths. **Not** an editor behavior change. | `.cargo/config.toml`, `script/build-release-cached` |
| **ast-grep dev helpers** | `ast-grep-helper.sh` wraps structural search with regex pre-filtering; `ast-grep-help-json.py` parses `ast-grep --help` into JSON schema. Speeds up agent-side code search pattern development. | `scripts/ast-grep-helper.sh`, `scripts/ast-grep-help-json.py` |
| **`sync-upstream.sh`** | Rebase local patches onto latest upstream and force-push `fork`. | `sync-upstream.sh` |

### What is *not* forked here

- Language-model endpoints, oaicopilot wiring, and `api_url: http://127.0.0.1:25100` live in **user settings** (`~/.config/zed/settings.json`) and sovereign deploy scripts — not in this git delta.
- Inference is **llama-swap** (toxicwind fork) on **:25100**. There is no vLLM.

### Build (this host)

```bash
cd /home/toxic/projects/zed
./script/build-release-cached
# or: cargo build --release -p zed -p cli
```

Requires `sccache` and `mold` on `PATH` (see `.cargo/config.toml`).

### Remotes

```text
origin   https://github.com/zed-industries/zed.git  (upstream, read-only)
fork     https://github.com/toxicwind/zed.git      (our patches, public)
```

---

### Installation

On macOS, Linux, and Windows you can [download Zed directly](https://zed.dev/download) or install Zed via your local package manager ([macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)).

Other platforms are not yet available:

- Web ([tracking discussion](https://github.com/zed-industries/zed/discussions/26195))

### Developing Zed

- [Building Zed for macOS](./docs/src/development/macos.md)
- [Building Zed for Linux](./docs/src/development/linux.md)
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

If you’d like to financially support the project, you can do so via GitHub Sponsors.
Sponsorships go directly to Zed Industries and are used as general company revenue.
There are no perks or entitlements associated with sponsorship.

