// `sortedBy` and `sortedByDescending` are stable: equal keys keep their original order.
struct Item {
    let name: String
    let rank: Int
}

func names(items: List<Item>) -> List<String> {
    return items.map { item => item.name }
}

func main() -> List<List<String>> {
    let items = [Item(name: "a", rank: 2), Item(name: "b", rank: 1), Item(name: "c", rank: 2), Item(name: "d", rank: 1)]
    return [names(items: items.sortedBy { item => item.rank }), names(items: items.sortedByDescending { item => item.rank })]
}
