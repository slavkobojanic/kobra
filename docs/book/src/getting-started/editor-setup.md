# Editor setup

The `k` binary includes a language server, `k lsp`. It is the compiler itself answering your editor, so
what the editor underlines is exactly what `k check` reports, and a rename means the same thing to the
editor as to the compiler. Any editor that speaks the Language Server Protocol can use it. The repository
has ready-made setups for Visual Studio Code, Neovim and Vim in `editors/`.

## What the editor can do

Open a project, the directory with `k.toml`, or any directory above it. Then:

- **Errors as you type.** Diagnostics appear a moment after you stop typing, with the compiler's notes and
  pointers to related code (the first declaration of a duplicate, the other end of a cycle). Unused imports
  and classes are greyed out.
- **Quick fixes.** Every error the compiler knows how to fix offers the fix: add a missing import, change
  `@Prop` to `@Binding`, add `try` or `await`, add a missing `match` arm. The fix that is safe to apply
  blindly is marked as preferred.
- **Navigation.** Go to the definition of anything, standard library included. Go to the *type* of a value.
  Go to the **implementations** of an interface or protocol, or of one of its methods: the shop example's
  `PaymentGateway` jumps to `PayboxCheckout`, `LocalCheckout` and `StripeCheckout`, and a protocol jumps
  to the service that serves it, in another app of the workspace too.
- **Find references and rename** across the whole project, imports and doc comment links included. Renaming
  a public declaration also renames its file, since a file's public declaration must share its name.
  Renaming an interface method renames the methods that implement it. Renaming a page renames its
  `.signIn`-style cases too. Standard library names can't be renamed.
- **Hover** shows the declaration and its documentation, with doc links you can click. On a `@Derived`
  field or an `@Effect` it lists the reactive values it reads, which are what make it re-run:

  ```k
  ~struct Todo {
  ~    let title: String
  ~}
  ~
  ~enum Filter {
  ~    case all
  ~    case open
  ~}
  ~
  ~extend List<Todo> {
  ~    func visible(filter: Filter) -> List<Todo> {
  ~        return self
  ~    }
  ~}
  ~
  ~component TodoList {
  ~    @Prop let todos: List<Todo>
  ~    @State var filter: Filter = .all
  @Derived let visibleTodos: List<Todo> = todos.visible(filter: filter)
  // Reactive dependencies: `filter`
  ~    view {
  ~        text { "{visibleTodos.count}" }
  ~    }
  ~}
  ```

  On a page it shows the page's URL. On a theme token (`.muted`) it shows the token's value and what each
  variant of the theme sets it to; on a class (`.class(.count)`) it shows the class's rule, and go to
  definition jumps to that rule.
- **Completion** lists only what is valid where you are: names in scope, members after `.`, the right
  implicit members after a bare `.` (`padding: .` lists your theme's spacing tokens, `filter = .` the cases
  of `Filter`), annotations after `@`. Choosing a declaration from another module adds its import.
  - After `.on(`, the events of the component or element before it, with their payload types and
    documentation: `CardView(card: card).on(` offers `edit:`, `button { … }.on(` offers `click:`. Events
    that an earlier `.on(…)` of the same chain already handles are left out.
  - In a `style` block: property names at the start of a declaration; after `property:` the keywords
    that property accepts (`justifyContent: ` offers `spaceBetween`, `center`, …) and the theme tokens of
    the right kind with their values (`.md` for `gap`, `.muted` for `color`); in a selector, the classes
    of the component; in `@media (`, the media features and their keywords.
  - In `.class(.`, `.class([.a, .`, or before `when`, the classes of the component's style block.
- **Signature help** inside a call, the **outline** of a file, **workspace symbols**, and **inlay hints**
  that show each page's URL next to its name when the URL comes from the page's name.

## Visual Studio Code

The extension is in `editors/vscode`. It needs `k` on your `PATH` (see [Installation](installation.md)), or
the `k.server.path` setting pointing at it.

```bash
cd editors/vscode
npm install
npx @vscode/vsce package
code --install-extension k-language-0.1.0.vsix
```

npm is only needed to package the extension; K itself never uses it. The command **K: Restart Language
Server** restarts the server, for example after you rebuild `k`.

Highlighting comes from the extension's own grammar, so after you update the repository, package and install
the extension again to pick up its fixes.

## Neovim

Add `editors/nvim` to the runtime path and call the setup function, for example with lazy.nvim:

```lua
{ dir = "/path/to/k/editors/nvim", config = function() require("k.lsp").setup() end }
```

It adds the `k` filetype, highlighting and comment settings, and starts `k lsp` for K buffers with the
nearest `k.toml` as the project root. It uses Neovim's built-in LSP configuration on 0.11 and later, and
nvim-lspconfig on older versions. To see page URLs inline, enable inlay hints:

```lua
vim.lsp.inlay_hint.enable(true)
```

## Vim

With [vim-lsp](https://github.com/prabirshrestha/vim-lsp) installed, add `editors/vim` to the runtime path:

```vim
Plug 'prabirshrestha/vim-lsp'
Plug '/path/to/k/editors/vim'
```

`:LspDefinition`, `:LspImplementation`, `:LspReferences`, `:LspRename`, `:LspHover` and `:LspCodeAction`
then work in `.k` files.

## Other editors

Configure the editor to run `k lsp` for `.k` files, talking over standard input and output, with the
directory containing `k.toml` as the root. The server needs nothing else: no configuration, no
environment, no network.

## When something looks wrong

- **No diagnostics at all**: check that the editor found `k` (`k --version` in the same environment) and
  that the file is inside a directory with `k.toml`. A `.k` file outside any project is checked on its
  own, so its imports can't resolve.
- **The editor and `k check` disagree**: they can't, unless the editor is running a different `k` than
  your terminal. Point the editor at the same binary.
- **Tracing**: in VS Code set `k.trace.server` to `messages` to see every request in the output panel.
