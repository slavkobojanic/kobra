import k.rpc.RpcOnlyError

/// An rpc whose implementation reads the config, so a view page can show
/// a config value without the client ever holding the config.
public protocol GreetingProtocol {
    rpc greeting() throws RpcOnlyError -> String
}
