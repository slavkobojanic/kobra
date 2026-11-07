-- The K language server for Neovim: `k lsp` over standard input and output.
--
--   require("k.lsp").setup()                 -- defaults
--   require("k.lsp").setup({ cmd = { "/path/to/k", "lsp" } })
--
-- On Neovim 0.11 and later this uses the built-in `vim.lsp.config` and
-- `vim.lsp.enable`; on older versions it registers a configuration with
-- nvim-lspconfig if that plugin is installed. Either way the server starts for
-- `k` buffers, rooted at the nearest directory holding `k.toml` (or `.git`).

local M = {}

--- The default configuration: `k lsp` from PATH for `k` buffers, rooted by
--- `root_markers` (the first found walking up from the buffer wins).
M.defaults = {
  cmd = { "k", "lsp" },
  filetypes = { "k" },
  root_markers = { "k.toml", ".git" },
}

--- Starts the K language server for K buffers.
--- @param opts table|nil overrides of `M.defaults` (any `vim.lsp.Config` field)
function M.setup(opts)
  local config = vim.tbl_deep_extend("force", M.defaults, opts or {})
  if vim.lsp.config and vim.lsp.enable then
    vim.lsp.config("k", config)
    vim.lsp.enable("k")
    return
  end
  local ok, configs = pcall(require, "lspconfig.configs")
  if not ok then
    vim.notify("k.lsp: needs Neovim 0.11+ or nvim-lspconfig", vim.log.levels.WARN)
    return
  end
  local util = require("lspconfig.util")
  if not configs.k then
    configs.k = {
      default_config = {
        cmd = config.cmd,
        filetypes = config.filetypes,
        root_dir = util.root_pattern(unpack(config.root_markers)),
        single_file_support = true,
      },
      docs = { description = "The K language server (`k lsp`)." },
    }
  end
  require("lspconfig").k.setup({})
end

return M
