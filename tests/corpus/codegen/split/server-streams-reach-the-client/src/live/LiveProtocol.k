import k.rpc.RpcOnlyError

public protocol LiveProtocol {
    rpc ticks() throws RpcOnlyError -> Stream<Tick>
    rpc counts() throws RpcOnlyError -> Stream<Int>
}
