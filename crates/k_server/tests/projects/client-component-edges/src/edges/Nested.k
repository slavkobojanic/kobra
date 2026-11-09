/// Client-rendered components nested in each other.
public page Nested at "/nested" {
    @State var after: String = ""

    view {
        Outer()

        textField(value: bind after)
            .label("After")
    }
}
