# Kobra for Vim

Filetype detection, syntax highlighting and the Kobra language server (`k lsp`) through
[vim-lsp](https://github.com/prabirshrestha/vim-lsp).

## Install

Add this directory to the runtime path, with a plugin manager or in `vimrc`:

```vim
" vim-plug
Plug 'prabirshrestha/vim-lsp'
Plug '/path/to/k/editors/vim'

" or by hand
set runtimepath+=/path/to/k/editors/vim
```

| File | What |
|---|---|
| `ftdetect/k.vim` | `.k` files get the `k` filetype |
| `ftplugin/k.vim` | `//` comments, `///` doc comment continuation, 4-space indentation |
| `syntax/k.vim` | highlighting (the same file as `editors/nvim/syntax/k.vim`) |
| `plugin/k_lsp.vim` | registers `k lsp` with vim-lsp for `k` buffers |

For a `k` that isn't on `PATH`:

```vim
let g:k_lsp_command = ['/path/to/k', 'lsp']
```

## Without the plugin file

The registration is a few lines in `vimrc`:

```vim
if executable('k')
  autocmd User lsp_setup call lsp#register_server({
        \ 'name': 'k',
        \ 'cmd': {server_info -> ['k', 'lsp']},
        \ 'allowlist': ['k'],
        \ })
endif
autocmd BufNewFile,BufRead *.k setfiletype k
```

vim-lsp's commands then work as usual: `:LspDefinition`, `:LspImplementation`, `:LspReferences`,
`:LspRename`, `:LspHover` (with reactive dependencies and page URLs), `:LspCodeAction` (the compiler's quick
fixes), `:LspDocumentDiagnostics`.
