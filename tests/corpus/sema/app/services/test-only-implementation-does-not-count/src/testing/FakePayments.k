// A test-only implementation.
import pay.PaymentProvider

public service FakePayments implements PaymentProvider {
    func charge(cents: Int) async {
    }
}
