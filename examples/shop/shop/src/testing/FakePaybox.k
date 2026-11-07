import pay.Charge
import pay.ChargeError
import pay.ChargeRequest
import pay.DeclineReason
import pay.Merchant
import pay.Payments
import pay.Refund
import pay.RefundError
import pay.RefundRequest
import pay.RegistrationError

/// A paybox that answers in-process, for tests that bind it with
/// `use FakePaybox for Payments`. It knows paybox's test cards, records
/// what it is asked, and charges each reference once.
public service FakePaybox implements Payments {
    /// Every callback URL registered, oldest first.
    var callbacks: List<Url> = []

    /// Every charge made, oldest first.
    var charged: List<ChargeRequest> = []

    rpc registerMerchant(callbackUrl: Url) async throws RegistrationError -> Merchant {
        callbacks.append(callbackUrl)

        return Merchant(name: "shop", callbackUrl: callbackUrl)
    }

    rpc charge(request: ChargeRequest) async throws ChargeError -> Charge {
        let number = request.card.number.replace(" ", with: "")

        if number == "4000000000000002" {
            throw ChargeError.declined(reason: DeclineReason.cardDeclined)
        }

        if number == "4000000000009995" {
            throw ChargeError.declined(reason: DeclineReason.insufficientFunds)
        }

        guard number.count == 16 else {
            throw ChargeError.invalidCard
        }

        if not charged.any({ earlier => earlier.reference == request.reference }) {
            charged.append(request)
        }

        return Charge(id: "ch_fake_{request.reference}", reference: request.reference, amount: request.amount, last4: "4242", refunded: Money.zero(request.amount.currency))
    }

    rpc refund(request: RefundRequest) async throws RefundError -> Refund {
        throw RefundError.unknownCharge
    }
}
