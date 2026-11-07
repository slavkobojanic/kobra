// Shared: usable on both sides.
public service FakePayments implements PaymentProvider {
    func charge(cents: Int) async {
    }
}
