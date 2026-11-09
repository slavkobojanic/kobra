import k.rpc.RpcOnlyError

public protocol GreeterProtocol {
    rpc greet(name: String) throws RpcOnlyError -> Greeting
    rpc fail(name: String) throws RpcOnlyError -> Greeting
}
