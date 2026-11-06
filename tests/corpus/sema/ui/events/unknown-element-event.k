// An event the element doesn't fire is K3105 at the label: `textField` has `focus`
// and `blur`, not `click`; `box` has `click` but not `submit`.
component Search {
    @State var query: String = ""

    func go() {
    }

    view {
        textField(value: bind query)
            .on(click: go)
            .on(focus: go)
        box { "x" }
            .on(submit: go)
            .on(mouseEnter: go)
    }
}
