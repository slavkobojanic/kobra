// `groupedBy` keeps the elements' order within a group and the groups in first-key order.
struct Order {
    let city: String
    let id: Int
}

func main() -> List<String> {
    let orders = [Order(city: "Oslo", id: 1), Order(city: "Bergen", id: 2), Order(city: "Oslo", id: 3)]
    let byCity = orders.groupedBy { order => order.city }
    var lines: List<String> = []
    for city in byCity.keys {
        let ids = (byCity[city] ?? []).map { order => "{order.id}" }
        lines.append("{city}: {ids.joined(separator: ",")}")
    }
    return lines
}
