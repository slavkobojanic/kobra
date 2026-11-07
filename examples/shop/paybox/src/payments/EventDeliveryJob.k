import k.observe.Log
import k.rpc.RpcFailure
import k.server.Job
import k.server.JobError
import merchants.MerchantRepository

/// Delivers one `pay.PaymentEvents` call to a merchant.
///
/// `PaymentsApi` enqueues it in the transaction that records the charge or
/// refund, so an event is delivered exactly when its change committed. A
/// merchant that is unreachable, too slow, refuses paybox's token or fails
/// is tried again with exponential backoff, starting at 2 seconds, for up to
/// 12 attempts (about two hours); after that the job is dead and
/// `dist/app jobs retry <id>` puts it back. Delivery is therefore at least
/// once, and merchants handle repeats.
///
/// The client is built when the job runs, from the merchant's current
/// callback URL, so a merchant that moved receives its pending events at
/// its new address.
@Retry(attempts: 12, backoff: 2.seconds)
@JobName("payment-event")
service EventDeliveryJob implements Job<EventDelivery> {
    @Inject let merchants: MerchantRepository
    @Inject let charges: ChargeRepository
    @Inject let refunds: RefundRepository
    @Inject let notifier: MerchantNotifier

    func run(payload: EventDelivery) async throws JobError {
        guard let merchant = await merchants.byName(payload.merchant) else {
            throw JobError.permanent(reason: "No merchant is called {payload.merchant}")
        }

        match payload.kind {
            case .chargeSucceeded {
                guard let charge = await charges.byId(payload.subject) else {
                    throw JobError.permanent(reason: "Charge {payload.subject} no longer exists")
                }

                if let failure = await notifier.chargeSucceeded(charge.toCharge(), at: merchant.callbackUrl) {
                    throw retry(payload: payload, failure: failure)
                }
            }

            case .chargeRefunded {
                guard let refund = await refunds.byId(payload.subject) else {
                    throw JobError.permanent(reason: "Refund {payload.subject} no longer exists")
                }

                if let failure = await notifier.chargeRefunded(refund.toRefund(), at: merchant.callbackUrl) {
                    throw retry(payload: payload, failure: failure)
                }
            }
        }

        Log.info("{payload.merchant} was told about {payload.subject}")
    }

    /// Logs a failed delivery and returns the error that schedules the next
    /// attempt.
    func retry(payload: EventDelivery, failure: RpcFailure) -> JobError {
        Log.warning("{payload.merchant} wasn't told about {payload.subject}: {describe(failure)}")

        return JobError.retryable(reason: "{payload.merchant} didn't take the event: {describe(failure)}")
    }

    /// A short description of why a delivery failed, for the job's log and
    /// its last error.
    func describe(_ failure: RpcFailure) -> String {
        return match failure {
            case .rejected { "the merchant refused paybox's service token" }

            case .invalid { "the merchant refused the event as invalid" }

            case .offline { "the merchant is unreachable" }

            case .timeout { "the merchant didn't answer in time" }
        }
    }
}
