import k.rpc.RpcOnlyError

public protocol GreeterProtocol {
    rpc greet(name: String) throws RpcOnlyError -> Greeting
}
