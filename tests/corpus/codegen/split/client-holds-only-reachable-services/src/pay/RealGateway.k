public service RealGateway implements Gateway {
    func start(amount: Int) async -> String {
        return "real-gateway-literal {amount}"
    }
}
