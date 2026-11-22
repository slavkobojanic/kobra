# Kobra for Claude Code

A [Claude Code](https://code.claude.com) plugin that teaches Claude to write correct, idiomatic Kobra.
It has three skills, which Claude loads on its own when you work with `.k` files, `k.toml` or the `k`
command:

| Skill | What it covers |
|---|---|
| `kobra-language` | syntax, types, optionals, typed errors, modules, components, views, reactivity, styles and themes, and the TypeScript, Swift and React habits that are compile errors in Kobra |
| `kobra-apps` | full-stack apps: `App.k`, pages and routing, services and injection, protocols and RPC, entities, migrations, `@Load`, forms, sign-in and guards, configs and secrets, jobs, email, HTTP, endpoint pages, streaming, workspaces, deployment |
| `kobra-tooling` | the `k` command, writing and running tests, reading and fixing diagnostics, lockfiles, formatting, editors |

It also registers the `kobra` MCP server (`k mcp`), which gives Claude the book, the language design,
the example apps and the compiler's checker (`search_docs`, `read_doc`, `read_example`,
`explain_diagnostic`, `check_snippet`). The skills tell Claude to check every piece of Kobra it writes
with `check_snippet`, or with `k check` when the server isn't available, and to format it with
`k fmt`.

## Requirements

`k` on your `PATH` (see `docs/book/src/getting-started/installation.md`), from a toolchain that has
the `k mcp` command. Without it the skills still work, but Claude falls back to running `k check` and
`k fmt` in your project.

## Installing

From inside Claude Code, add this repository as a plugin marketplace and install the plugin:

```text
/plugin marketplace add slavkobojanic/kobra
/plugin install kobra@kobra
```

Or from a shell:

```bash
claude plugin marketplace add slavkobojanic/kobra
claude plugin install kobra@kobra
```

Add `--scope project` to the install command to record the plugin in the project's
`.claude/settings.json`, so everyone working on the project gets it.

To try the plugin from a local checkout without installing it, run `claude --plugin-dir`:

```bash
claude --plugin-dir /path/to/kobra/editors/claude
```

To pick up a newer version of the skills, run `claude plugin marketplace update kobra`.

## Working on Kobra itself

In this repository the skills are also linked into `.claude/skills/`, so Claude Code uses them in
this repository without the plugin. Claude Code connects to the MCP server only through the plugin, or
through `claude mcp add kobra -- k mcp` (see `docs/book/src/getting-started/ai-assistants.md`).

## Layout

```text
editors/claude/
  .claude-plugin/plugin.json      the plugin manifest (name "kobra")
  .mcp.json                       starts `k mcp` as the `kobra` server
  skills/<skill>/SKILL.md         each skill's entry point, kept short
  skills/<skill>/references/*.md  detail Claude reads when it needs it
```

The marketplace manifest is `.claude-plugin/marketplace.json` at the repository root. Every Kobra code
block in the skills is an excerpt of a project that passed `k check` and `k fmt --check`. When you
change a skill, check its samples the same way. `claude plugin validate editors/claude` checks the
manifest.
