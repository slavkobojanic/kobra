" The K language server for Vim, through vim-lsp
" (https://github.com/prabirshrestha/vim-lsp): `k lsp` over standard input
" and output for `k` buffers, rooted at the nearest directory with `k.toml`.
"
" Set g:k_lsp_command to use a `k` that isn't on PATH:
"
"   let g:k_lsp_command = ['/path/to/k', 'lsp']

if exists('g:loaded_k_lsp')
  finish
endif
let g:loaded_k_lsp = 1

function! s:k_root(server_info) abort
  let l:manifest = findfile('k.toml', expand('%:p:h') . ';')
  return lsp#utils#path_to_uri(l:manifest ==# '' ? expand('%:p:h') : fnamemodify(l:manifest, ':p:h'))
endfunction

function! s:register() abort
  if !exists('*lsp#register_server')
    return
  endif
  call lsp#register_server({
        \ 'name': 'k',
        \ 'cmd': {server_info -> get(g:, 'k_lsp_command', ['k', 'lsp'])},
        \ 'root_uri': function('s:k_root'),
        \ 'allowlist': ['k'],
        \ })
endfunction

augroup k_lsp
  autocmd!
  autocmd User lsp_setup call s:register()
augroup END
