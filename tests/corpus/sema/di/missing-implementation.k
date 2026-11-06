interface Payments {
    func charge(cents: Int) async
}

service Checkout {
    @Inject let payments: Payments
}
