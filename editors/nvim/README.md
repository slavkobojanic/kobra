# K for Neovim

Filetype detection, syntax highlighting and the K language server (`k lsp`) for Neovim. The server is the
compiler's own analysis, so diagnostics, quick fixes, navigation, rename, hover, completion, signature help,
symbols and inlay hints match `k check` exactly.

## Install

Add this directory to the runtime path, with any plugin manager or by hand:

```lua
-- lazy.nvim
{ dir = "/path/to/k/editors/nvim", config = function() require("k.lsp").setup() end }

-- or plainly, in init.lua
vim.opt.runtimepath:append("/path/to/k/editors/nvim")
require("k.lsp").setup()
```

It provides:

| File | What |
|---|---|
| `ftdetect/k.lua` | `.k` files get the `k` filetype |
| `ftplugin/k.lua` | `//` comments, `///` doc comment continuation, 4-space indentation |
| `syntax/k.vim` | highlighting (the same file as `editors/vim/syntax/k.vim`) |
| `lua/k/lsp.lua` | `require("k.lsp").setup(opts)` starts `k lsp` for `k` buffers |

`setup` uses `vim.lsp.config`/`vim.lsp.enable` on Neovim 0.11+, and nvim-lspconfig on older versions.
Options override the defaults (`cmd = { "k", "lsp" }`, `root_markers = { "k.toml", ".git" }`); for a `k`
that isn't on `PATH`:

```lua
require("k.lsp").setup({ cmd = { vim.fn.expand("~/src/k/target/release/k"), "lsp" } })
```

## nvim-lspconfig only

Without this directory, the server can be registered with nvim-lspconfig directly:

```lua
local configs = require("lspconfig.configs")
if not configs.k then
  configs.k = {
    default_config = {
      cmd = { "k", "lsp" },
      filetypes = { "k" },
      root_dir = require("lspconfig.util").root_pattern("k.toml", ".git"),
      single_file_support = true,
    },
  }
end
require("lspconfig").k.setup({})
vim.filetype.add({ extension = { k = "k" } })
```

Useful mappings once the server is attached: `gd` (definition), `gri` (implementation), `grr`
(references), `grn` (rename), `gra` (code actions, for the compiler's quick fixes), `K` (hover, with the
reactive dependencies of `@Derived` fields and `@Effect`s, and page URLs). Inlay hints, which show each
page's derived URL, are enabled with `vim.lsp.inlay_hint.enable(true)`.
