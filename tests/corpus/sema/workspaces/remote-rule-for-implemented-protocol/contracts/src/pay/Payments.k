/// Takes payments.
public protocol Payments {
    /// Charges a card.
    rpc charge(cents: Int) throws ChargeError -> String
}
