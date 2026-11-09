/// Calls the greeter for the page.
public service GreeterClient {
    @Inject let greeter: GreeterProtocol

    func greeting(name: String) async -> String {
        do {
            let greeting = try await greeter.greet(name: name)
            return shout(word: greeting.text)
        } catch .rpcFailure(let failure) {
            return "failed"
        }
    }
}
