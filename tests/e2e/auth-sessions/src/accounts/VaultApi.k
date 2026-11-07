import k.auth.Session
import k.rpc.RpcOnlyError

/// The server side of `VaultProtocol`.
public service VaultApi implements VaultProtocol {
    @Inject let session: Session

    rpc note() async throws RpcOnlyError -> String {
        return "the vault note for {session.user.id.name}"
    }
}
