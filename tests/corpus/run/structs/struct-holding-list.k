// A struct's list field is copied with the struct: appending through one copy leaves the other alone.
struct Bag {
    var items: List<String>
}

func main() -> List<List<String>> {
    let original = Bag(items: ["a"])
    var copy = original
    copy.items.append("b")
    var third = copy
    third.items.removeAll()
    return [original.items, copy.items, third.items]
}
