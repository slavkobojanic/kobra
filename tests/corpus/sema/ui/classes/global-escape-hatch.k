// `global("…")` is a class from global CSS, outside scoping; it mixes with
// typed classes in a list.
component Pay {
    view {
        button { "Pay" }
            .class(global("btn-primary"))
            //     ^^^^^^^^^^^^^^^^^^^^^ type: Classes
        button { "Cancel" }
            .class([.ghost, global("btn")])
    }

    style {
        .ghost { opacity: 0.8; }
    }
}
