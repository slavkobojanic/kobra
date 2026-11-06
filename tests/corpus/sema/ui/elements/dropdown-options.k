// `dropdown(selection:)` holds `option`s whose values have the selection's type.
// An `Int` option in a `String` dropdown is K2001; a paragraph is K3107.
component Picker {
    @State var size: String = "m"

    view {
        dropdown(selection: bind size) {
            option(value: "s") { "Small" }
            option(value: "m") { "Medium" }
            option(value: 3) { "Large" }
            paragraph { "x" }
        }
    }
}
