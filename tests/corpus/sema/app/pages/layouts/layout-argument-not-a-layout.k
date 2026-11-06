// `@Layout` takes a layout's type name.
service CartService {
    @State var count: Int = 0
}

@Layout(CartService)
page Home {
    view {
        text { "home" }
    }
}
