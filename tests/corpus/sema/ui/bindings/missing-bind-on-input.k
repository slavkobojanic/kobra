// Inputs take a `Binding<T>` too: `textField(value: name)` misses `bind` (K3016).
component Signup {
    @State var name: String = ""
    @State var agreed: Bool = false

    view {
        textField(value: name)
        checkbox(isChecked: bind agreed)
    }
}
