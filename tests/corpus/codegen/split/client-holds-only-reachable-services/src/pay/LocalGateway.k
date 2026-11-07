public service LocalGateway implements Gateway {
    func start(amount: Int) async -> String {
        return "local-gateway-literal {amount}"
    }
}
