// A bodiless requirement's signature may wrap; the next member starts on its own line.
interface PaymentProvider {
    func charge(
        amount: Money,
        card: CardToken,
    )
        async
        throws PaymentError
        -> Receipt
    func cancel()
}
