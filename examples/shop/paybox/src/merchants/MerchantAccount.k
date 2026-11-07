import k.crypto.Uuid
import pay.Merchant

/// A merchant service that registered with paybox, stored in the
/// `merchant_account` table.
public entity MerchantAccount {
    /// The primary key.
    @Id let id: Uuid

    /// The merchant's name: the caller name of its service token, which is
    /// its `[package] name`. Charges and refunds belong to this name.
    @Unique let name: String

    /// Where the merchant serves `PaymentEvents`. Updated whenever the
    /// merchant registers again.
    var callbackUrl: Url

    /// When the merchant first registered.
    let registeredAt: Date

    /// The merchant as `Payments` returns it.
    func toMerchant() -> Merchant {
        return Merchant(name: name, callbackUrl: callbackUrl)
    }
}
