// filter, map, sortedBy (stable), first and count.
struct Item {
    let name: String
    let price: Int
}

func main() -> List<String> {
    let items = [Item(name: "b", price: 2), Item(name: "a", price: 1), Item(name: "c", price: 2)]
    let names = items
        .filter { item => item.price > 1 }
        .sortedBy { item => item.price }
        .map { item => item.name }
    let cheapest = items.sortedBy { item => item.price }.first?.name ?? "none"
    return names + [cheapest, "{items.count(where: { item => item.price == 2 })}"]
}
