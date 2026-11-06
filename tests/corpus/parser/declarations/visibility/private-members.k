// `private` may prefix fields, functions, initializers and rpcs.
service CartService implements CartProtocol {
    private var retryCount = 0
    private let limit: Int = 3
    private func reset() {}
    private init() {}
    private rpc list() -> List<Item> {}
}
