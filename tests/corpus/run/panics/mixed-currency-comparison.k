// Ordering Money across currencies panics.
func cheaper(_ a: Money, _ b: Money) -> Bool {
    return a < b
}

func main() -> Bool {
    return cheaper(5.gbp, 6.usd)
}
