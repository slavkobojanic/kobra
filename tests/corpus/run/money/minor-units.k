// `minorUnits` and `Money(minorUnits:currency:)` convert exactly, by the currency's digits.
func main() -> List<String> {
    let fromCents = Money(minorUnits: 2499, currency: .usd)
    let fromYen = Money(minorUnits: 1500, currency: .jpy)
    return ["{24.00.usd.minorUnits}", "{1500.jpy.minorUnits}", "{fromCents == 24.99.usd}", "{fromYen == 1500.jpy}", "{0.01.usd.minorUnits}"]
}
