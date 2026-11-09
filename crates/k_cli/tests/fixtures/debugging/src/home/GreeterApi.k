import k.rpc.RpcOnlyError

public service GreeterApi implements GreeterProtocol {
    rpc greet(name: String) async throws RpcOnlyError -> Greeting {
        let shout = name.uppercased()
        return Greeting(text: "Hello, {shout}")
    }

    rpc fail(name: String) async throws RpcOnlyError -> Greeting {
        let none: List<Greeting> = []
        return none[name.count]
    }
}
