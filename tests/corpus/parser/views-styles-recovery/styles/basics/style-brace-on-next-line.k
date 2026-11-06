// The lexer looks past newlines and comments after `style` for the `{` that enters style mode.
component T {
    style // the styles
    {
        .a { color: red; }
    }
}
