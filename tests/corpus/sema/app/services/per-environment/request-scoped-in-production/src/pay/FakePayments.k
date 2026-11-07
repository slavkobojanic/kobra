// App-scoped (the default).
public service FakePayments implements PaymentProvider {
    func charge(cents: Int) async {
    }
}
