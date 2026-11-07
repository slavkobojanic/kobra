/// An exact decimal amount plus a [`Currency`] (`docs/design/34-money.md`).
///
/// Literals are number extensions per ISO 4217 code: `24.00.usd`. `+` and
/// `-` require one currency, and `Money * Int` scales by a quantity.
@Native
public struct Money implements Equatable, Hashable, Comparable, Codable {
    /// Creates an amount from minor units:
    /// `Money(minorUnits: 2400, currency: .usd)` is `24.00.usd`.
    @Native
    init(minorUnits: Int, currency: Currency) {}

    /// The amount in minor units (`24.00.usd` is `2400`); see
    /// [`Currency.minorUnitDigits`].
    var minorUnits: Int
    /// The amount's currency.
    var currency: Currency

    /// Returns zero in `currency`.
    static func zero(_ currency: Currency) -> Money

    /// Scales the amount by `rate`, rounded back to the currency's digits.
    ///
    /// - Parameters:
    ///   - rate: the factor, such as a tax rate (`1.0825`).
    ///   - rounding: how a result between two minor units is rounded
    ///     ([`RoundingRule`]).
    /// - Returns: the scaled amount, in the same currency.
    func multiplied(by rate: Decimal, rounding: RoundingRule) -> Money
    /// Splits the amount into `parts` amounts that add up to it exactly;
    /// panics when `parts` is less than 1.
    ///
    /// - Returns: `parts` amounts in the same currency, differing by at
    ///   most one minor unit.
    func allocated(into parts: Int) -> List<Money>
    /// Formats the amount, as `style` says ([`NumberStyle`]);
    /// [`NumberStyle.currency`] uses the locale in effect.
    func formatted(as style: NumberStyle) -> String
}

/// Money literals on whole numbers: `10.eur`, `1500.jpy`. One property per
/// ISO 4217 code; the codes listed are the cases of [`Currency`] declared
/// here.
public extend Int {
    /// This many US dollars.
    @Native
    var usd: Money
    /// This many euros.
    @Native
    var eur: Money
    /// This many pounds sterling.
    @Native
    var gbp: Money
    /// This many yen.
    @Native
    var jpy: Money
    /// This many Swiss francs.
    @Native
    var chf: Money
}

/// Money literals with a fraction: `24.00.usd`. More decimal places than the
/// currency has (`1.005.usd`) is a compile error.
public extend Double {
    /// This many US dollars.
    @Native
    var usd: Money
    /// This many euros.
    @Native
    var eur: Money
    /// This many pounds sterling.
    @Native
    var gbp: Money
    /// This many yen.
    @Native
    var jpy: Money
    /// This many Swiss francs.
    @Native
    var chf: Money
}
