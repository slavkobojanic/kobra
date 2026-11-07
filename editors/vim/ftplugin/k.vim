" Buffer settings for K: comments and the canonical indentation of `k fmt`.
if exists("b:did_ftplugin")
  finish
endif
let b:did_ftplugin = 1

setlocal commentstring=//\ %s
setlocal comments=:///,://
setlocal expandtab shiftwidth=4 tabstop=4 softtabstop=4

let b:undo_ftplugin = "setlocal commentstring< comments< expandtab< shiftwidth< tabstop< softtabstop<"
