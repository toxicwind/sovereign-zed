#!/usr/bin/env python3
"""Parse `ast-grep <cmd> --help` into a structured JSON schema."""
import subprocess, json, re, sys

def run_help(args):
    r = subprocess.run(args, capture_output=True, text=True, timeout=5)
    return r.stdout + r.stderr

def parse_subcommands(text):
    cmds = {}
    in_cmd = False
    for ln in text.split('\n'):
        if ln.strip().startswith('Commands:'):
            in_cmd = True; continue
        if in_cmd and ln.strip().startswith('Options:'):
            break
        if in_cmd:
            m = re.match(r'^  (\w[\w-]*)\s{2,}(.*)$', ln)
            if m:
                cmds[m.group(1)] = m.group(2).strip()
    return cmds

def parse_sections(text):
    """Split help text into: description (before Usage), usage, arguments, options."""
    lines = text.split('\n')

    desc_lines = []
    usage_lines = []
    arg_lines = []
    opt_lines = []
    section = 'desc'

    for ln in lines:
        if section == 'desc':
            if ln.strip().startswith('Usage:'):
                section = 'usage'
                usage_lines.append(ln)
            else:
                desc_lines.append(ln)
        elif section == 'usage':
            usage_lines.append(ln)
            if ln.strip().startswith('Arguments:'):
                section = 'args'
            elif ln.strip().startswith('Options:'):
                section = 'options'
        elif section == 'args':
            if ln.strip().startswith('Options:'):
                section = 'options'
                opt_lines.append(ln)
            else:
                arg_lines.append(ln)
        elif section == 'options':
            opt_lines.append(ln)

    return {
        'description': '\n'.join(desc_lines).strip(),
        'usage': '\n'.join(usage_lines).strip(),
        'args_text': '\n'.join(arg_lines).strip(),
        'options_text': '\n'.join(opt_lines).strip()
    }

def parse_options(opt_text):
    """Each option: {long, short, value, description}."""
    options = []
    lines = opt_text.split('\n')
    i = 0
    while i < len(lines):
        ln = lines[i]
        m = re.match(r'^( {2,})(-[\w], )?(--[\w][\w-]*)([ =<\[].*)$', ln)
        if m:
            short = m.group(2).rstrip(', ') if m.group(2) else None
            long = m.group(3)
            raw_val = m.group(4).strip()

            val = None
            if raw_val.startswith('<') and raw_val.endswith('>'):
                val = raw_val[1:-1]
            elif raw_val.startswith('[=<') and raw_val.endswith('>]'):
                val = raw_val[3:-2]
            elif raw_val.startswith('[') and raw_val.endswith(']'):
                inner = raw_val[1:-1]
                val = inner if inner else None

            # Collect description lines
            desc_parts = []
            j = i + 1
            while j < len(lines):
                nxt = lines[j]
                if re.match(r'^( {2,})(-[\w], )?(--[\w])', nxt):
                    break
                stripped = nxt.strip()
                if stripped:
                    desc_parts.append(stripped)
                j += 1

            desc = ' '.join(desc_parts)
            options.append({'long': long, 'short': short, 'value': val, 'description': desc})
            i = j
        else:
            i += 1
    return options

def parse_args(arg_text):
    """Parse [PATHS]... style positional arguments."""
    if not arg_text.strip():
        return []
    args = []
    lines = arg_text.split('\n')
    current = None
    for ln in lines:
        m = re.match(r'^  \[?(\w[\w\[\].-]*)\]?\s*$', ln)
        if m:
            if current:
                args.append(current)
            current = {'name': m.group(1), 'description': ''}
        elif current and ln.startswith('          ') and ln.strip():
            current['description'] += (' ' if current['description'] else '') + ln.strip()
    if current:
        args.append(current)
    return args

# ── Main ──
root_help = run_help(['ast-grep', '--help'])
subcommands = parse_subcommands(root_help)

all_opts_by_long = {}
cmd_defs = {}

for cmd_name in subcommands:
    cmd_help = run_help(['ast-grep', cmd_name, '--help'])
    sections = parse_sections(cmd_help)
    opts = parse_options(sections['options_text'])
    args = parse_args(sections['args_text'])

    cmd_defs[cmd_name] = {
        'description': subcommands[cmd_name],
        'usage': sections['usage'],
        'args': args,
        'options': opts
    }

    for o in opts:
        key = o['long']
        if key not in all_opts_by_long:
            all_opts_by_long[key] = []
        all_opts_by_long[key].append(cmd_name)

# Global: appears in >=3 commands
global_opts = {k for k, v in all_opts_by_long.items() if len(v) >= 3}

result = {'global_options': {}, 'commands': {}}
for cmd_name, cmd_def in cmd_defs.items():
    cmd_opts = []
    for o in cmd_def['options']:
        if o['long'] in global_opts:
            if o['long'] not in result['global_options']:
                result['global_options'][o['long']] = {
                    'short': o['short'],
                    'value': o['value'],
                    'description': o['description'],
                    'appears_in': all_opts_by_long[o['long']]
                }
        else:
            cmd_opts.append(o)
    cmd_def['options'] = cmd_opts
    result['commands'][cmd_name] = cmd_def

# Write to file to avoid truncation, then cat first block
with open('/tmp/astgrep_schema.json', 'w') as f:
    json.dump(result, f, indent=2)
print(f"Wrote {sys.getsizeof(json.dumps(result))} chars to /tmp/astgrep_schema.json", file=sys.stderr)
