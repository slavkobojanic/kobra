/// An ISO 4217 currency (`docs/design/34-money.md`). The cases listed here
/// are those the examples use; every ISO 4217 code is a case.
public enum Currency implements Equatable, Hashable, Codable {
    /// US dollar.
    case usd
    /// Euro.
    case eur
    /// Pound sterling.
    case gbp
    /// Japanese yen.
    case jpy
    /// Swiss franc.
    case chf

    /// The upper-case code, such as `"USD"`.
    @Native
    var code: String
    /// The number of minor-unit digits (2 for USD, 0 for JPY).
    @Native
    var minorUnitDigits: Int

    /// Parses an upper-case code; `nil` if it isn't a known currency.
    @Native
    static func parse(code: String) -> Currency?
}
