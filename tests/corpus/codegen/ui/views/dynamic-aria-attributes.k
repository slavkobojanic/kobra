// `.pressed(x)` and `.expanded(x)` with reactive values are attribute bindings on `aria-pressed`
// and `aria-expanded`; ARIA state is the text `true`/`false`, never removed for `false`
// (05-views.md, codegen.md §11.1).
component Toggle {
    @State var isOn: Bool = false
    @State var isOpen: Bool = false

    view {
        box {
            button { "Bold" }
                .pressed(isOn)
                .on(click: { isOn = not isOn })

            button { "Menu" }
                .expanded(isOpen)
                .on(click: { isOpen = not isOpen })
        }
    }
}
