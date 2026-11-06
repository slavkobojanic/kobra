// Classes are scoped: a class of another component's style block is not this
// component's class.
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card { padding: 4px; }
    }
}

component Home {
    view {
        box { "y" }
            .class(.card)
    }
}
