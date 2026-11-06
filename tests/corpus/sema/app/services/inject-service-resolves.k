// A component injects a service: the field's type and the uses of the field resolve to it.
service CartService {
    @State var count: Int = 0
}

component CartBadge {
    @Inject let cart: CartService
    //                ^^^^^^^^^^^ resolves: decl corpus.CartService

    view {
        text { "{cart.count}" }
        //       ^^^^ type: corpus.CartService
        //            ^^^^^ resolves: member corpus.CartService.count
    }
}
