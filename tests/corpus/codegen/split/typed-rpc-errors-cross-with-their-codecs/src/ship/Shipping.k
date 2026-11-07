/// The pages' way to the shipping rpcs.
public service Shipping {
    @Inject let api: ShippingProtocol

    func book(day: Int) async throws ShippingError -> String {
        return try await api.book(day: day)
    }

    func count() async throws PlainError -> Int {
        return try await api.count()
    }
}
