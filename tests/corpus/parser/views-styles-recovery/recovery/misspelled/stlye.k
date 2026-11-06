// A misspelled `style` does not enter style mode, so its body is lexed as code; it is skipped as one K0126 ErrorNode.
component T {
    stlye {
        .a { color: red; }
    }

    view {}
}
