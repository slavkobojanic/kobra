// An interface with exactly one implementation is injected automatically, without a `use` rule.
interface Payments {
    func charge(cents: Int) async
}

service CardPayments implements Payments {
    func charge(cents: Int) async {
    }
}

service Checkout {
    @Inject let payments: Payments

    func pay() async {
        await payments.charge(cents: 100)
        //    ^^^^^^^^ type: corpus.Payments
    }
}
