// `multiplied(by:rounding:)` rounds back to the currency's digits with the given rule.
func main() -> List<Money> {
    let total = 10.05.usd
    return [
        total.multiplied(by: 0.5, rounding: .halfEven),
        total.multiplied(by: 0.5, rounding: .halfUp),
        total.multiplied(by: 0.5, rounding: .down),
        total.multiplied(by: 0.5, rounding: .up),
        10.15.usd.multiplied(by: 0.5, rounding: .halfEven),
        100.usd.multiplied(by: 1.0825, rounding: .halfEven),
    ]
}
