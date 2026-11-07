import k.rpc.ServiceAuthenticated

/// Takes card payments for the merchants of the system. Served by paybox
/// and called by merchant services such as the shop, never by a browser:
/// `ServiceAuthenticated` admits only callers holding a service token signed
/// with paybox's secret, and the token's caller name is the merchant's name.
///
/// Paybox is a stand-in for a real payment provider, so it never moves
/// money. Like a provider's test mode, it decides by card number:
///
/// | Card number | Outcome |
/// |---|---|
/// | `4242 4242 4242 4242` | succeeds |
/// | `4000 0000 0000 0002` | `.declined(.cardDeclined)` |
/// | `4000 0000 0000 9995` | `.declined(.insufficientFunds)` |
/// | `4000 0000 0000 0069` | `.declined(.expiredCard)` |
/// | any other number that passes the Luhn check | succeeds |
/// | anything else | `.invalidCard` |
///
/// Every outcome a merchant must act on also arrives later, and at least
/// once, through `PaymentEvents`, which the merchant serves at the callback
/// URL it registered.
public protocol Payments requires ServiceAuthenticated {
    /// Registers the calling merchant, or updates its callback URL. Calling
    /// it again with the same URL changes nothing, so a merchant may call it
    /// before every checkout.
    ///
    /// - Parameter callbackUrl: the base URL where the merchant serves
    ///   `PaymentEvents`, as paybox reaches it (an internal address, not
    ///   necessarily the merchant's public origin).
    /// - Returns: the merchant as paybox now knows it.
    /// - Throws: `RegistrationError.unknownCaller` when the call didn't come
    ///   with a service token, and `.rpcFailure` when paybox refused the
    ///   token, was unreachable or didn't answer in time.
    rpc registerMerchant(callbackUrl: Url) throws RegistrationError -> Merchant

    /// Charges a card.
    ///
    /// Charging is idempotent per merchant and `request.reference`: a
    /// repeated request for a reference that was already charged returns the
    /// first charge and charges nothing. A declined card leaves no charge, so
    /// the customer can try another one.
    ///
    /// - Parameter request: what to charge, to which card.
    /// - Returns: the charge, already succeeded.
    /// - Throws: `ChargeError.unknownMerchant` before `registerMerchant`,
    ///   `.invalidAmount`, `.invalidCard` or `.declined` as described above,
    ///   and `.rpcFailure` when paybox refused the service token, was
    ///   unreachable or didn't answer in time.
    rpc charge(request: ChargeRequest) throws ChargeError -> Charge

    /// Refunds part or all of a charge. Each call is a new refund; the
    /// refunds of a charge never add up to more than its amount.
    ///
    /// - Parameter request: the charge and the amount to give back.
    /// - Returns: the refund.
    /// - Throws: `RefundError.unknownCharge` when the calling merchant has
    ///   no charge with this id, `.invalidAmount` for an amount that isn't
    ///   positive or is in another currency, `.exceedsCharge` when the
    ///   amount is more than what is left to refund, and `.rpcFailure` as
    ///   for `charge`.
    rpc refund(request: RefundRequest) throws RefundError -> Refund
}
