/// A completed charge.
public struct Charge implements Codable {
    /// The charge's id, `ch_<order>`.
    let id: String
    /// The order it paid for.
    let orderId: String
    /// The amount charged, in cents.
    let cents: Int
    /// The service that asked for it, as its service token named it.
    let caller: String
}
