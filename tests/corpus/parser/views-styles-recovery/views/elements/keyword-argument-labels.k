// Argument labels may be keywords (`to`, `for`, `as`, `in`); the label keeps its keyword text.
component T {
    view {
        link(to: .home) { "Home" }

        label(for: "email") { "Email" }

        text { total.formatted(as: .currency) }

        Group(in: section) {}
    }
}
