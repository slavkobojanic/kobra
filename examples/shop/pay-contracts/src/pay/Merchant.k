/// A merchant registered with paybox.
public struct Merchant implements Codable, Equatable {
    /// The merchant's name: the `[package] name` of the service that
    /// registered, as its service token names it.
    let name: String

    /// Where paybox delivers the merchant's `PaymentEvents`.
    let callbackUrl: Url
}
