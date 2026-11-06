// Stray operators between members are skipped as one ErrorNode with K0126 at the first token.
component T {
    + - *
    view {}
}
