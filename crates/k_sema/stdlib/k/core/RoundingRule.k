/// How `Money.multiplied(by:rounding:)` rounds back to the currency's
/// digits.
public enum RoundingRule {
    /// To the nearest; ties to the even digit (banker's rounding).
    case halfEven
    /// To the nearest; ties away from zero.
    case halfUp
    /// Toward zero.
    case down
    /// Away from zero.
    case up
}
