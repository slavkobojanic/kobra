// `.class([.panel, .expanded when isExpanded])` keeps the static class in the template and binds
// the conditional one with `classList` (codegen.md §11.1).
component Accordion {
    @State var isExpanded: Bool = false

    view {
        box { "details" }
            .class([.panel, .expanded when isExpanded])
            .on(click: { isExpanded = not isExpanded })
    }

    style {
        .panel { overflow: hidden; }
        .expanded { maxHeight: 400px; }
    }
}
