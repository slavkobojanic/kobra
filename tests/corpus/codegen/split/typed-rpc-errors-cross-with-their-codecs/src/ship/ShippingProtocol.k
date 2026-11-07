public protocol ShippingProtocol {
    rpc book(day: Int) throws ShippingError -> String
    rpc count() throws PlainError -> Int
}
