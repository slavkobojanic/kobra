import k.rpc.RpcOnlyError

public protocol WorkProtocol {
    rpc record(text: String, delayMs: Int) throws RpcOnlyError -> Bool
    rpc recordInTransaction(text: String) throws RpcOnlyError -> Bool
    rpc recordThenRollBack(text: String) throws RpcOnlyError -> Bool
    rpc flaky(text: String) throws RpcOnlyError -> Bool
    rpc doomed(text: String) throws RpcOnlyError -> Bool
    rpc refused(text: String) throws RpcOnlyError -> Bool
    rpc entries() throws RpcOnlyError -> List<String>
}
