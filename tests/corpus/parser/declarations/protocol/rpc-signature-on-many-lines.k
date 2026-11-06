// Each part of an rpc signature may start a new line.
protocol CartProtocol {
    rpc add(
        request: AddItemRequest,
    )
        async
        throws CartError
        -> CartItem
        requires Authenticated,
            RateLimit(10.perSecond)
    rpc next()
}
