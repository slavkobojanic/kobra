// Pseudo-elements use two adjacent colons.
component T {
    style {
        .input::placeholder { color: .muted; }
        .a::after { content: ""; }
    }
}
