import k.rpc.RpcOnlyError

public protocol GreetProtocol {
    rpc greet(name: String) throws RpcOnlyError -> Greeting
}
