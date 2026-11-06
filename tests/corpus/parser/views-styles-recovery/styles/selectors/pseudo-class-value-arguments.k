// Other pseudo-classes take a CSS value; unit suffixes such as `n` are not validated there.
component T {
    style {
        .row:nth-child(2n + 1) {}
        .row:nth-child(odd) {}
        .row:nth-of-type(-n + 3) {}
    }
}
