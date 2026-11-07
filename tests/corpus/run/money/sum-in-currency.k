// `sum(in:)` adds Money in one currency and is Money.zero for an empty list
// (docs/design/34-money.md).
struct Line {
    let price: Money
    let quantity: Int
}

func main() -> List<Money> {
    let lines = [Line(price: 2.50.usd, quantity: 2), Line(price: 1.25.usd, quantity: 1)]
    let none: List<Line> = []
    return [
        lines.sum(in: .usd) { line => line.price * line.quantity },
        none.sum(in: .eur) { line => line.price },
    ]
}
