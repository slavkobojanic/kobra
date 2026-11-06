// Without a style block a component has no classes: any reference is K3203.
component Plain {
    view {
        box { "x" }
            .class(.card)
    }
}
