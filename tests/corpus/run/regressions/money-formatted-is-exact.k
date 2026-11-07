// `Money.formatted` passes `Number(amount)` to `Intl.NumberFormat`, so an
// amount past 2^53 minor units is rounded through a double: "Money is never
// a floating-point number, and it never silently ... rounds" (34-money.md).
func main() -> String {
    return 90071992547409.93.usd.formatted(as: .currency)
}
