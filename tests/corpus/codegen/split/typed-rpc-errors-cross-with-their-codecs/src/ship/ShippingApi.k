public service ShippingApi implements ShippingProtocol {
    rpc book(day: Int) async throws ShippingError -> String {
        throw ShippingError.closed
    }

    rpc count() async throws PlainError -> Int {
        throw PlainError.refused(count: 1)
    }
}
