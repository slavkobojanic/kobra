import k.rpc.RpcOnlyError

public protocol NotesProtocol {
    rpc find(title: String) throws RpcOnlyError -> List<NoteSummary>
}
