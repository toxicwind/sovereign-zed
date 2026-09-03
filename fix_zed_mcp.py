#!/usr/bin/env python3
"""Rerunnable Python script with monadic case-pattern to fix/check zed MCP persistence."""
import subprocess, sys, os, json, time
from pathlib import Path

# Always absolute — no pwd dependency
ZED_DIR = Path("/home/toxic/projects/zed")
UPSTREAM_FIX = "604221dbc7"

# Monadic case-pattern dispatch (Python match/case like Scala cases)
class M:
    @staticmethod
    def bind(x): return x

def case_run(cmd, label, ok_codes=(0,)):
    """Monadic 'case' for shell actions with rerunnable idempotency."""
    result = subprocess.run(cmd, capture_output=True, text=True, cwd=str(ZED_DIR))
    status = "OK" if result.returncode in ok_codes else "FAIL"
    return {
        "label": label,
        "status": status,
        "returncode": result.returncode,
        "stdout": result.stdout[-500:] if len(result.stdout) > 500 else result.stdout,
        "stderr": result.stderr[-500:] if len(result.stderr) > 500 else result.stderr,
    }

def main():
    actions = []

    # Case 1: verify cherry-pick applied (check cx.notify present)
    store_path = ZED_DIR / "crates/project/src/context_server_store.rs"
    content = store_path.read_text()
    has_notify = "cx.notify();" in content

    match has_notify:
        case True:
            actions.append(case_run(
                ["git", "rev-parse", "--short", "HEAD"],
                "verify_fix_applied"
            ))
        case False:
            actions.append(case_run(
                ["git", "cherry-pick", "--no-commit", UPSTREAM_FIX],
                "apply_upstream_fix"
            ))

    # Case 2: rerunnable trace log
    actions.append({
        "label": "python_trace_log",
        "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
        "zed_dir": str(ZED_DIR.resolve()),
        "upstream_fix": UPSTREAM_FIX,
        "notify_present": has_notify,
    })

    # Print monadic result
    print(json.dumps({
        "rerunnable": True,
        "pwd_fixed": True,
        "path": str(ZED_DIR.resolve()),
        "case_result": actions[-1]["label"] if isinstance(actions[-1], dict) else actions[-1]["label"],
        "actions": actions,
    }, indent=2))
    return 0 if all(isinstance(a, dict) and a.get("status") != "FAIL" for a in actions if isinstance(a.get("status"), str)) else 1

if __name__ == "__main__":
    sys.exit(main())
