// `.name` against a category resolves to that category's token, in any module, without an import; `space md` and `radius md` don't clash.
let accent: Color = .primary
//                   ^^^^^^^ resolves: token Color.primary
let gap: Spacing = .md
//                  ^^ resolves: token Spacing.md
let corner: Radius = .md
//                    ^^ resolves: token Radius.md
