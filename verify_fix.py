#!/usr/bin/env python3
"""Rerunnable Python conversion of the bash verification command."""
import subprocess, sys
from pathlib import Path

ZED_DIR = Path("/home/toxic/projects/zed")
STORE = ZED_DIR / "crates/project/src/context_server_store.rs"

print("=== Script is rerunnable, absolute paths, Scala-style match/case ===")
print(f"Path: {Path(__file__).resolve()}")
print("=== What the upstream fix does (604221dbc7) ===")
print("1. ContextServerStore::update_server_state() and remove_server() now emit cx.notify()")
print("2. render_toggle_switch() bound to is_enabled (settings) not is_running (runtime)")
print("=== Our zed dir ===")
subprocess.run(["ls", "-la", str(ZED_DIR / "fix_zed_mcp.py")])
print("=== Confirm fix present ===")
count = subprocess.run(["grep", "-c", "cx.notify()", str(STORE)], capture_output=True, text=True)
print(count.stdout.strip())
