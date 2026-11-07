// `textField(value: bind name)` sets the `value` property from the signal and writes the signal on
// `input`; the server renders the `value` attribute (05-views.md, "Inputs bind their value").
component Profile {
    @State var name: String = ""

    view {
        textField(value: bind name)
            .label("Name")
    }
}
