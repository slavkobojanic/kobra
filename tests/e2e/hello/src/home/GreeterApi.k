import k.rpc.RpcOnlyError

public service GreeterApi implements GreeterProtocol {
    rpc greet(name: String) async throws RpcOnlyError -> Greeting {
        return Greeting(text: "Hello, {name}")
    }
}
