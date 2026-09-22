# Dispatching Claude Code as a subagent

The repository can delegate a whole workstream to **Claude Code** (the
`claude` CLI) in headless mode. Claude Code brings its own agent harness
(tools, subagents, skills, MCP) and its model routing lives in
`~/.claude/settings.json`; the current setup points `ANTHROPIC_BASE_URL`
at DeepSeek, so switching the harness to real Claude later is a settings
change, not a code change.

## Usage

```
tools/claude_code.sh -f prompt.md [--model M] [--timeout S] [--max-turns N]
tools/claude_code.sh --safe "one-shot prompt"
tools/claude_code.sh -f prompt.md --append-system "extra system text"
```

- `-f FILE` reads the prompt from a file; positionally you can pass the
  prompt inline. Write batch prompts the same way the opencode subagent
  prompts are written: goal, scope, constraints, the exact verification
  commands and the report format.
- `--safe` uses `acceptEdits` instead of `bypassPermissions` (bash asks);
  the default is fully autonomous in the repo, so only use it for prompts
  you trust and always with a timeout.
- Raw results are written to `build/claude-code/<timestamp>.json` with
  stderr next to them; the script prints duration, turns, cost and the
  tail of the result. `build/` is gitignored.

## Configuration and limits

- Model routing: `~/.claude/settings.json` `env` keys
  (`ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_MODEL`,
  `CLAUDE_CODE_SUBAGENT_MODEL`). Point them at Anthropic (or a
  Claude-capable router) to use Claude; `--model` overrides per run.
- The `[1m]` suffix on the current model id triggers an
  `unrecognized_model` warning in headless mode; pass `--model
  deepseek-flash` if that matters.
- Claude Code does not know this repository's conventions by itself:
  every prompt must carry the rules (English only, <=300-line files,
  docs-first, the CONFIG_*/minimal invariants, licensing firewall, the
  smoke suite and the known riscv flakes). The orchestrator models its
  prompts on the ones given to opencode subagents.
- One harness per workstream: the orchestrator verifies and commits the
  result (Claude Code is asked not to commit or push), keeping the commit
  trail and evidence under the project's rules.

## When to use which

- opencode subagent (`coder`): small/medium batches, tight integration
  with the orchestrator's todo and verification loop.
- Claude Code (`tools/claude_code.sh`): large, long-running workstreams
  where a second full harness helps, or when the harness is switched to a
  stronger model than the orchestrator's.
