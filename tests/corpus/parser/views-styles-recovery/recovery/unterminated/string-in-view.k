// An unterminated string in a text node is K0002 (lexer) only; the string ends at the line end and the `}` on the next line closes the block.
component T {
    view {
        text { "abc
        }
    }
}
