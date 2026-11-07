// A job runs without a session; in production its payments read one.
import k.server.Job
import k.server.JobError
import pay.PaymentProvider

public service ChargeJob implements Job<ChargeRequest> {
    @Inject let payments: PaymentProvider

    func run(payload: ChargeRequest) async throws JobError {
    }
}
