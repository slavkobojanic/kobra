" Vim syntax file for K (.k files).
"
" The same file serves Vim (editors/vim) and Neovim (editors/nvim); the two
" copies must stay identical, and must list every keyword of the compiler's
" lexer: crates/k_lsp/tests/editors.rs checks both. Semantic information
" (diagnostics, navigation, hover) comes from the language server, `k lsp`.

if exists("b:current_syntax")
  finish
endif

let s:cpo_save = &cpo
set cpo&vim

" Hard keywords: never identifiers.
syn keyword kConditional if else guard match case default when
syn keyword kRepeat for in while break continue
syn keyword kStatement return throw try catch do await expect
syn keyword kOperatorWord and or not is as bind
syn keyword kStorage let var func init rpc import struct enum class interface extend style
syn keyword kModifier public private static async throws implements requires where use
syn keyword kBoolean true false
syn keyword kConstant nil
syn keyword kSelf self

" Contextual keywords: keywords where a declaration or block starts.
syn keyword kDeclaration app component config entity layout migration page protocol repository service test theme
syn keyword kBlock view metadata loading failed variant proto
syn keyword kContextual at key matches

" Comments: `///` is documentation, with ``[`Name`]`` links.
syn keyword kTodo contained TODO FIXME XXX NOTE
syn match kDocLink contained "\[`[^`\]]\+`\]"
syn match kDocCode contained "`[^`]\+`"
syn region kDocComment start="///" end="$" contains=kDocLink,kDocCode,kTodo,@Spell
syn region kLineComment start="//\(/\)\@!" end="$" contains=kTodo,@Spell
syn region kBlockComment start="/\*" end="\*/" contains=kBlockComment,kTodo,@Spell

" Strings with `{interpolation}`, escapes and `{{`.
syn match kEscape contained "\\[nrt0\\\"]\|\\u{\x\{1,6}}\|{{"
syn region kInterpolation contained matchgroup=kInterpolationDelimiter start="{" end="}" contains=TOP
syn region kString start=+"+ skip=+\\"+ end=+"+ contains=kEscape,kInterpolation

" Numbers, units and colors.
syn match kColor "#\x\{3,8}\>"
syn match kNumber "\<\d[0-9_]*\(\.\d\+\)\=\([eE][+-]\=\d\+\)\=\>"
syn match kNumber "\<0x[0-9A-Fa-f_]\+\>\|\<0b[01_]\+\>\|\<0o[0-7_]\+\>"
syn match kUnit "\<\d[0-9_]*\(\.\d\+\)\=\(px\|rem\|em\|vh\|vw\|vmin\|vmax\|ch\|ex\|fr\|ms\|s\|deg\|rad\|turn\|%\)"

" Annotations, types and declarations.
syn match kAnnotation "@\h\w*"
syn match kType "\<\u\w*\>"
syn match kFunction "\<\l\w*\ze\s*("
syn match kMember "?\=\.\zs\l\w*"

hi def link kConditional Conditional
hi def link kRepeat Repeat
hi def link kStatement Statement
hi def link kOperatorWord Operator
hi def link kStorage StorageClass
hi def link kModifier StorageClass
hi def link kBoolean Boolean
hi def link kConstant Constant
hi def link kSelf Identifier
hi def link kDeclaration Structure
hi def link kBlock Keyword
hi def link kContextual Keyword
hi def link kTodo Todo
hi def link kDocComment SpecialComment
hi def link kDocLink Underlined
hi def link kDocCode Special
hi def link kLineComment Comment
hi def link kBlockComment Comment
hi def link kString String
hi def link kEscape SpecialChar
hi def link kInterpolationDelimiter Delimiter
hi def link kColor Constant
hi def link kNumber Number
hi def link kUnit Number
hi def link kAnnotation PreProc
hi def link kType Type
hi def link kFunction Function
hi def link kMember Identifier

let b:current_syntax = "k"

let &cpo = s:cpo_save
unlet s:cpo_save
