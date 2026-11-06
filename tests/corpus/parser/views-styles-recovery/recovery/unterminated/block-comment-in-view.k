// An unterminated block comment swallows the rest of the file: K0003 at `/*` and K0101 for each open brace.
component T {
    view {
        box {} /* never closed
    }
}
