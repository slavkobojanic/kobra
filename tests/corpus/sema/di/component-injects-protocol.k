import k.rpc.RpcOnlyError

protocol CartProtocol {
    rpc items() throws RpcOnlyError -> List<Int>
}

service CartService {
    @Inject let api: CartProtocol
}

component Cart {
    @Inject let api: CartProtocol
    @Inject let cart: CartService

    view {
        text { "cart" }
    }
}
