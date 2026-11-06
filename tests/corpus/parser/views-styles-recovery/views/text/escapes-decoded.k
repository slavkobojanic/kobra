// Escapes in text nodes are decoded into one atom; `{{` is a literal brace and `}` needs no escape.
component T {
    view {
        text { "Tab\tQuote\"Slash\\ Brace{{x}" }
    }
}
