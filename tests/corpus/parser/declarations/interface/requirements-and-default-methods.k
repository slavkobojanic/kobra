// Bodiless functions are requirements; functions with bodies are defaults.
interface PaymentProvider {
    func charge(amount: Money, card: CardToken) async throws PaymentError -> Receipt

    func supports(currency: Currency) -> Bool {
        return currency == .usd
    }

    func refund(receipt: Receipt) async throws PaymentError
}
