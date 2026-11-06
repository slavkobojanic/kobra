// Pages, like components, never inject protocols; remote calls go through services.
import k.rpc.RpcOnlyError

protocol CartProtocol {
    rpc items() throws RpcOnlyError -> List<Int>
}

page Cart {
    @Inject let api: CartProtocol

    view {
        text { "cart" }
    }
}
