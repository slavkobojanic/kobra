// `contains(where:)` beside `contains(_:)` (sema regression
// `list-contains-where-overload.k`): the labelled overload is the one called,
// on both outcomes, and `contains(_:)` still compares elements.
struct Order {
    let total: Int
}

func main() -> String {
    let names = ["ada", "bo"]
    let orders = [Order(total: 5), Order(total: 12)]
    let a = names.contains(where: { s => s.count > 2 })
    let b = names.contains(where: { s => s.count > 3 })
    let c = names.contains("bo")
    let d = orders.contains(where: { o => o.total > 10 })
    return "{a} {b} {c} {d}"
}
