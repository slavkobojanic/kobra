// Attributes are per element: `box` has no `.disabled` and no `.pressed`
// (05-views.md: "`.pressed` on a `box` is a compile error"), `checkbox` has no
// `.placeholder`.
component Panel {
    @State var isOn: Bool = false

    view {
        box { "x" }
            .disabled(true)
            .pressed(true)
        checkbox(isChecked: bind isOn)
            .placeholder("tick")
            .label("On")
    }
}
