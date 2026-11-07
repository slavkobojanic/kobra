// `@Inject` is a lookup in the scope's container, `ctx.c` (codegen.md §10, §13).
service CartService {
    @State var count: Int = 0
}

component CartBadge {
    @Inject let cart: CartService

    view {
        text { "{cart.count}" }
    }
}
