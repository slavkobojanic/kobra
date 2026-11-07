// `sum(in:)` panics when an element is in another currency.
struct Line {
    let price: Money
}

func main() -> Money {
    let lines = [Line(price: 1.usd), Line(price: 2.eur)]
    return lines.sum(in: .usd) { line => line.price }
}
