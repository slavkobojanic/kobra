/// How [`Money.multiplied(by:rounding:)`] rounds back to the currency's
/// digits.
public enum RoundingRule {
    /// Rounds to the nearest; ties to the even digit (banker's rounding).
    case halfEven
    /// Rounds to the nearest; ties away from zero.
    case halfUp
    /// Rounds toward zero.
    case down
    /// Rounds away from zero.
    case up
}
