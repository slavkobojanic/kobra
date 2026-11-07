// Equality includes the currency; ordering within one currency compares amounts.
func main() -> List<Bool> {
    return [48.00.usd == 48.00.usd, 48.00.usd == 48.00.eur, 1.usd == 1.00.usd, 1.99.usd < 2.usd, 5.eur >= 5.eur, Money.zero(.usd) == 0.usd]
}
