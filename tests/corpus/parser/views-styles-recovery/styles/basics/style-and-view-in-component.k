// A component with both a view and a style block; `.class(.label)` in the view is code, `.label` in the style is a selector.
component T {
    view {
        text { "Hi" }
            .class(.label)
    }

    style {
        .label { fontSize: .lg; }
    }
}
