import k.http.HttpResponse
import k.http.IncomingRequest
import k.observe.Log
import payments.StripeWebhookVerifier
import store.PaymentRecorder

/// The endpoint Stripe calls when something happens to a payment.
///
/// It is an endpoint page: it has a handler instead of a `view`, so it
/// answers raw HTTP and is compiled into the server bundle only. Endpoint
/// pages exist only for callers that can't speak RPC, and always have a
/// fixed URL; this one is registered in the Stripe dashboard, so renaming
/// the page must never move it. It doesn't inject `k.auth.Session`, so it
/// receives no cookie and needs no request-forgery check.
///
/// Responses follow Stripe's retry rules. Anything Stripe can't fix by
/// sending again (a recorded payment, an ignored event, an unknown order)
/// gets `204 No Content`, so Stripe stops redelivering it. Only requests that
/// fail verification get `400 Bad Request`.
public page StripeWebhook at "/webhooks/stripe" {
    @Inject let verifier: StripeWebhookVerifier
    @Inject let recorder: PaymentRecorder

    /// Verifies the delivery, then applies it to its order.
    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        let signature = request.headers["stripe-signature"]

        let event = try await verifier.verify(payload: request.rawBody, signatureHeader: signature) else {
            return HttpResponse(status: .badRequest)
                .body(.text("Webhook signature verification failed."))
        }

        match event {
            case .checkoutPaid(let orderId, let amount, let reference) {
                let outcome = await recorder.recordPayment(orderId: orderId, amount: amount, reference: reference)

                match outcome {
                    case .recorded {
                        Log.info("Order {orderId} paid ({reference})")
                    }

                    case .notAwaitingPayment {
                        Log.info("Order {orderId} is not awaiting payment; ignoring delivery")
                    }

                    case .amountMismatch(let expected, let received) {
                        Log.error("Order {orderId} expected {expected} but Stripe collected {received}; flagged for review")
                    }

                    case .unknownOrder {
                        Log.error("Stripe reported payment {reference} for unknown order {orderId}")
                    }
                }
            }

            case .ignored(let type) {
                Log.debug("Ignoring Stripe event {type}")
            }
        }

        return HttpResponse(status: .noContent)
    }
}
