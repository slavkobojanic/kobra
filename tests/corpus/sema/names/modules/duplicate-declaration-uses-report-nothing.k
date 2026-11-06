// After a K1004, uses of the name resolve without further diagnostics.
struct Item {
    let id: Int
}

struct Item {
    let id: Int
}

func identify(item: Item) -> Int {
    return item.id
}
