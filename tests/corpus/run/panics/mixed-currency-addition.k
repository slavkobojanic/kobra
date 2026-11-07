// Adding Money in two currencies panics when the compiler can't see both currencies.
func add(_ a: Money, _ b: Money) -> Money {
    return a + b
}

func main() -> Money {
    return add(1.usd, 1.eur)
}
