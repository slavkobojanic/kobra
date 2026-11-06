// Layouts never inject protocols.
import k.rpc.RpcOnlyError

protocol CartProtocol {
    rpc items() throws RpcOnlyError -> List<Int>
}

layout Shop {
    @Inject let api: CartProtocol
    @Slot let content: View

    view {
        content
    }
}
