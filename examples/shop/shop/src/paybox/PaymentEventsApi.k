import k.crypto.Uuid
import k.observe.Log
import k.rpc.RpcOnlyError
import pay.Charge
import pay.PaymentEvents
import pay.Refund
import store.PaymentRecorder

/// The shop's side of `pay.PaymentEvents`: paybox calls it, from a
/// background job, after each charge and refund.
///
/// Deliveries arrive at least once, so handling is idempotent:
/// `PaymentRecorder` marks an order paid with a conditional update, and a
/// repeated event changes nothing and sends no second email. Every outcome
/// is final, so every delivery is acknowledged; only the ones that need a
/// person are logged as errors.
public service PaymentEventsApi implements PaymentEvents {
    @Inject let recorder: PaymentRecorder

    rpc chargeSucceeded(charge: Charge) async throws RpcOnlyError {
        guard let orderId = Uuid.parse(charge.reference) else {
            Log.error("Paybox reported charge {charge.id} for {charge.reference}, which isn't an order of this shop")

            return
        }

        let outcome = await recorder.recordPayment(orderId: orderId, amount: charge.amount, reference: charge.id)

        match outcome {
            case .recorded {
                Log.info("Order {orderId} paid ({charge.id})")
            }

            case .notAwaitingPayment {
                Log.info("Order {orderId} is not awaiting payment; ignoring delivery of {charge.id}")
            }

            case .amountMismatch(let expected, let received) {
                Log.error("Order {orderId} expected {expected} but paybox charged {received}; flagged for review")
            }

            case .unknownOrder {
                Log.error("Paybox reported charge {charge.id} for unknown order {orderId}")
            }
        }
    }

    rpc chargeRefunded(refund: Refund) async throws RpcOnlyError {
        // Refunds are made by staff in paybox; the shop only records that
        // one happened, for support.
        Log.info("Paybox refunded {refund.amount} of order {refund.reference} ({refund.id})")
    }
}
