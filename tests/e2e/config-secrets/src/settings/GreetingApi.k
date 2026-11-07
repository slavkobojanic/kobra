import k.rpc.RpcOnlyError

/// The server side of `GreetingProtocol`.
public service GreetingApi implements GreetingProtocol {
    @Inject let config: AppConfig

    rpc greeting() async throws RpcOnlyError -> String {
        return "{config.greeting} (token {config.token})"
    }
}
