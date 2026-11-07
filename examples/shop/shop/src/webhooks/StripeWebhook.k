import k.http.HttpResponse
import k.http.IncomingRequest
import k.observe.Log
import payments.PaymentEvent
import payments.StripeEvents
import store.PaymentRecorder

/// The endpoint Stripe calls when something happens to a payment, when the
/// optional Stripe binding is enabled in `App.k`; otherwise it answers
/// `404 Not Found` to every delivery.
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
///
/// Payments through paybox, the default, never come here: paybox calls the
/// shop's `PaymentEvents` rpc instead (`paybox.PaymentEventsApi`), and both
/// paths end in `PaymentRecorder`.
public page StripeWebhook at "/webhooks/stripe" {
    @Inject let verifier: StripeEvents
    @Inject let recorder: PaymentRecorder

    /// Verifies the delivery, then applies it to its order.
    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        let signature = request.headers["stripe-signature"]

        do {
            let event = try await verifier.verify(payload: request.rawBody, signatureHeader: signature)

            await apply(event: event)
        } catch .disabled {
            return HttpResponse(status: .notFound)
                .body(.text("This shop doesn't take payments through Stripe."))
        } catch let error {
            Log.debug("Refused a Stripe webhook delivery: {error}")

            return HttpResponse(status: .badRequest)
                .body(.text("Webhook signature verification failed."))
        }

        return HttpResponse(status: .noContent)
    }

    /// Applies a verified event to its order.
    func apply(event: PaymentEvent) async {
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
    }
}
