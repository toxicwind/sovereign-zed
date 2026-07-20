#!/usr/bin/env bash
# ast-grep-helper -- Structural search with regex pre-filter
# ast-grep patterns are AST trees, NOT regex. This wrapper lets you mix both.
#
# Usage: source ast-grep-helper.sh  (or run ./ast-grep-helper.sh)
set -euo pipefail

SG="ast-grep"

# ── Internal: detect if pattern needs regex pre-filter ──
_is_regex() {
    local p="$1"
    # Contains regex metacharacters beyond what ast-grep supports
    [[ "$p" =~ [\|\(\)\{\}\[\]\^\$\.\*\+] ]] && return 0
    return 1
}

# ── Internal: filter path list by content regex ──
_rg_filter() {
    local regex="$1"; shift
    rg -l "$regex" "$@" 2>/dev/null || true
}

# ── Find structs (regex or exact name) ──
function ag-find-struct {
    local name="${1:?Usage: ag-find-struct <name> [lang=rust] [path=.]}"
    local lang="${2:-rust}"
    local path="${3:-.}"

    if _is_regex "$name"; then
        # Pre-filter: find files containing struct declaration matching regex
        local files; files=$(_rg_filter "struct.*${name}" "$path")
        if [[ -z "$files" ]]; then return 0; fi
        echo "$files" | while read -r f; do
            $SG run -l "$lang" -p 'struct $A { $$$ }' "$f" 2>/dev/null \
                | rg "$name"
        done
    else
        $SG run -l "$lang" -p "struct $name { \$\$\$ }" "$path" 2>/dev/null
    fi
}

# ── Find functions (regex or exact name) ──
function ag-find-fn {
    local name="${1:?Usage: ag-find-fn <name> [lang=rust] [path=.]}"
    local lang="${2:-rust}"
    local path="${3:-.}"

    if _is_regex "$name"; then
        local files; files=$(_rg_filter "fn ${name}" "$path")
        if [[ -z "$files" ]]; then return 0; fi
        echo "$files" | while read -r f; do
            $SG run -l "$lang" -p 'fn $A($$$) -> $B { $$$ }' "$f" 2>/dev/null \
                | rg "$name"
        done
    else
        $SG run -l "$lang" -p "fn $name(\$\$\$) -> \$\$A { \$\$\$ }" "$path" 2>/dev/null || \
        $SG run -l "$lang" -p "fn $name(\$\$\$)" "$path" 2>/dev/null
    fi
}

# ── Find impl blocks (regex or exact trait) ──
function ag-find-impl {
    local name="${1:?Usage: ag-find-impl <Trait> [lang=rust] [path=.]}"
    local lang="${2:-rust}"
    local path="${3:-.}"

    if _is_regex "$name"; then
        local files; files=$(_rg_filter "impl ${name}" "$path")
        if [[ -z "$files" ]]; then return 0; fi
        echo "$files" | while read -r f; do
            $SG run -l "$lang" -p 'impl $A for $B { $$$ }' "$f" 2>/dev/null \
                | rg "$name"
        done
    else
        $SG run -l "$lang" -p "impl $name for \$\$A { \$\$\$ }" "$path" 2>/dev/null
    fi
}

# ── Find any struct (pre-filter by regex if given) ──
function ag-find-all-structs {
    local regex="${1:-}"
    local lang="${2:-rust}"
    local path="${3:-.}"

    if [[ -n "$regex" ]]; then
        local files; files=$(_rg_filter "struct.*${regex}" "$path")
        if [[ -z "$files" ]]; then return 0; fi
        echo "$files" | while read -r f; do
            $SG run -l "$lang" -p 'struct $A { $$$ }' "$f" 2>/dev/null \
                | rg "$regex"
        done
    else
        $SG run -l "$lang" -p 'struct $A { $$$ }' "$path" 2>/dev/null
    fi
}

# ── Find any match with AST pattern + optional content regex ──
function ag-find {
    local pattern="${1:?Usage: ag-find <ast_pattern> [content_regex] [lang=rust] [path=.]}"
    local content_regex="${2:-}"
    local lang="${3:-rust}"
    local path="${4:-.}"

    if [[ -n "$content_regex" ]]; then
        $SG run -l "$lang" -p "$pattern" "$path" 2>/dev/null | rg "$content_regex"
    else
        $SG run -l "$lang" -p "$pattern" "$path" 2>/dev/null
    fi
}

# ── Quick outline (regex-filtered symbols) ──
function ag-outline {
    local lang="${1:-rust}"
    local path="${2:-.}"
    local regex="${3:-}"

    if [[ -n "$regex" ]]; then
        $SG outline -l "$lang" --view signatures "$path" 2>/dev/null | rg "$regex"
    else
        $SG outline -l "$lang" --view signatures "$path" 2>/dev/null
    fi
}

# ── Generate schema JSON ──
function ag-schema {
    python3 /home/toxic/Zed/ast-grep-help-json.py 2>/dev/null && echo "/tmp/astgrep_schema.json"
}

echo "[ast-grep-helper] loaded: ag-find-fn ag-find-struct ag-find-impl ag-find ag-outline ag-schema" >&2