# AI assistants (MCP)

The `k` binary includes a server for AI coding assistants, `k mcp`. It speaks the Model Context Protocol
(MCP), so Claude Code and any other MCP client can ask it about Kobra while it writes Kobra for you. It
answers from this book, the language design, the compiler's documentation and the example apps, all built
into `k`, and it checks code with the compiler itself, so an assistant can test what it wrote before showing
it to you.

## Installing it in Claude Code

With `k` on your `PATH` (see [Installation](installation.md)):

```bash
claude mcp add kobra -- k mcp
```

Run it inside a project to use the server there, or add `--scope user` after `add` to use it everywhere.
Start Claude Code again, and `/mcp` lists the `kobra` server and its tools.

Other MCP clients take the same command: `k mcp`, talking over standard input and output. A client that
reads a JSON configuration file is set up like this:

```json
{
  "mcpServers": {
    "kobra": { "command": "k", "args": ["mcp"] }
  }
}
```

The server needs nothing else: no network, no account and no configuration. Everything it says about Kobra
is the documentation of the `k` that runs it, so it matches the compiler you have.

## What the assistant can do

| Tool | What it does |
|---|---|
| `search_docs` | searches the book, the design, the compiler docs, the examples and your project for words, best match first |
| `list_docs`, `read_doc` | lists the documents and reads one, a page of lines at a time |
| `list_examples`, `read_example` | lists the example apps and reads one: a file, or all its sources at once |
| `explain_diagnostic` | explains a diagnostic code such as `K2011`: what it means, an example and the fix, as in the [diagnostics reference](../reference/diagnostics.md) |
| `check_snippet` | checks Kobra code with the same analysis `k check` runs and returns its diagnostics |

The documents are also MCP resources, at addresses such as `kobra://book/language/types.md` and
`kobra://examples/todos/App.k`, for clients that let you attach a resource to a conversation.

`check_snippet` checks one file, with any other files of the same program it is given. A file declaring an
`app` is checked as `App.k`, any other as a module file named after its first public declaration, unless the
assistant names the path. It runs no tests and no lockfile checks; for those, the assistant runs `k check`
in your project, as you would.

## Your project

When `k mcp` starts inside a project, the directory with `k.toml` or any directory below it, the server also
serves that project's `.k` sources, `k.toml` and Markdown files, read from disk on every request, so the
assistant can search your code beside the documentation. Name another project with `--root`:

```bash
k mcp --root ../shop
```

The server reads files and never writes any.
