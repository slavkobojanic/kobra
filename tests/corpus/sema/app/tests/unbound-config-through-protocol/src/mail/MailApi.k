import k.rpc.RpcOnlyError

// The server side, which needs the config.
public service MailApi implements MailProtocol {
    @Inject let config: MailConfig

    rpc send(to: String) throws RpcOnlyError {}
}
