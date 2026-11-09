import k.rpc.RpcOnlyError

// What the browser asks the server.
public protocol MailProtocol {
    rpc send(to: String) throws RpcOnlyError
}
