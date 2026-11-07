// Rounding rules on negative amounts: `down` goes toward zero, `up` away from it.
func main() -> List<Money> {
    let refund = 0.usd - 10.05.usd
    return [refund.multiplied(by: 0.5, rounding: .down), refund.multiplied(by: 0.5, rounding: .up), refund.multiplied(by: 0.5, rounding: .halfUp)]
}
