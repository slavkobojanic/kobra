import k.crypto.Uuid

/// The `Payment` page's entry point for card payments.
///
/// Pages inject this service, never `CardProtocol`, as the layering rules
/// require. In the browser, `api` is a generated Connect client of the
/// shop's own server.
public service CardService {
    @Inject let api: CardProtocol

    /// Charges an order's total to a card.
    ///
    /// - Throws: every `CardPaymentError`; see
    ///   `CardProtocol.pay(orderId:card:)`.
    func pay(orderId: Uuid, card: CardForm) async throws CardPaymentError {
        try await api.pay(orderId: orderId, card: card)
    }
}
