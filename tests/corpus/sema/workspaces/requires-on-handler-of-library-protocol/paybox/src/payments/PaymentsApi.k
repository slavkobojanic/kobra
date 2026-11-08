import k.auth.RateLimit
import pay.ChargeError
import pay.Payments

/// The implementation.
public service PaymentsApi implements Payments {
    rpc charge(cents: Int) async throws ChargeError -> String requires RateLimit(5.perMinute) {
        return "ok"
    }
}
