import pay.ChargeError
import pay.ChargeRequest
import pay.Payments

/// Pays through paybox. Server-only, since it holds a remote client.
public service Checkout {
    @Inject let payments: Payments

    /// Charges `cents` for `orderId`; the charge's id.
    func pay(orderId: String, cents: Int) async throws CheckoutError -> Receipt {
        do {
            let charge = try await payments.charge(request: ChargeRequest(orderId: orderId, cents: cents))
            return Receipt(chargeId: charge.id, cents: charge.cents)
        } catch ChargeError.declined {
            throw CheckoutError.declined
        } catch ChargeError.rpcFailure(_) {
            throw CheckoutError.paymentsUnavailable
        }
    }
}
