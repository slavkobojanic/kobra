import pay.ChargeError
import pay.ChargeRequest
import pay.Payments

/// Pays for orders through paybox.
public service Checkout {
//             ^^^^^^^^ side: server-only
    @Inject let payments: Payments

    func pay(cents: Int) async throws ChargeError -> String {
        let charge = try await payments.charge(request: ChargeRequest(cents: cents, card: "tok"))
        //                              ^^^^^^ resolves: member pay.Payments.charge(request:)
        return charge.id
    }
}
