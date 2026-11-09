/// A client-rendered component between server-rendered siblings that load.
public page Siblings at "/siblings" {
    @State var search: String = ""

    view {
        textField(value: bind search)
            .label("Search")

        Stock(name: "Apples")

        Basket()

        Stock(name: "Pears")
    }
}
