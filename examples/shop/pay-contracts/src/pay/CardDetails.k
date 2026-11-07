/// The card a charge is made to.
///
/// Paybox is a test provider, so only the number matters (see `Payments`);
/// a real provider would also take the expiry date and the security code,
/// and a real merchant would never see the number at all.
public struct CardDetails implements Codable {
    /// The card number. Spaces and dashes between the digits are ignored.
    let number: String
}
