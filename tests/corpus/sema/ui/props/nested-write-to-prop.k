// Writing a field of a struct prop writes the prop: K3014 at the whole place.
struct Item {
    var name: String
}

component Row {
    @Prop let item: Item

    func rename() {
        item.name = "x"
    }

    view {
        text { item.name }
    }
}
