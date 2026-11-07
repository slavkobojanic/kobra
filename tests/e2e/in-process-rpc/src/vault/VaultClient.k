import k.rpc.RpcFailure

/// What the page shows: each rpc's outcome as text. During server
/// rendering `vault` is the in-process client.
public service VaultClient {
    @Inject let vault: VaultProtocol

    func describe(failure: RpcFailure) -> String {
        match failure {
            case .rejected(.unauthenticated) { return "rejected unauthenticated" }
            case .rejected(.forbidden) { return "rejected forbidden" }
            case .rejected(.rateLimited(let wait)) { return "rejected rate limited {wait.inSeconds > 0}" }
            case .invalid(let error) { return "invalid {error.failures[0].path}: {error.failures[0].message}" }
            case .offline { return "offline" }
            case .timeout { return "timeout" }
        }
    }

    func report() async -> String {
        var lines: List<String> = []

        do {
            lines = lines.appending("secret: {try await vault.secret()}")
        } catch .rpcFailure(let f) {
            lines = lines.appending("secret: {describe(failure: f)}")
        }

        do {
            lines = lines.appending("admin: {try await vault.admin()}")
        } catch .rpcFailure(let f) {
            lines = lines.appending("admin: {describe(failure: f)}")
        }

        do {
            lines = lines.appending("blank: {try await vault.check(credentials: Credentials(name: " "))}")
        } catch .rpcFailure(let f) {
            lines = lines.appending("blank: {describe(failure: f)}")
        }

        do {
            lines = lines.appending("named: {try await vault.check(credentials: Credentials(name: "ada"))}")
        } catch .rpcFailure(let f) {
            lines = lines.appending("named: {describe(failure: f)}")
        }

        return lines.joined(separator: "; ")
    }

    func limited() async -> String {
        do {
            return try await vault.limited()
        } catch .rpcFailure(let f) {
            return describe(failure: f)
        }
    }
}
