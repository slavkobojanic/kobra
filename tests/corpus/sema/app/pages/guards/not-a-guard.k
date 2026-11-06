// `requires` names guards only: types that implement `Guard`.
service CartService {
    @State var count: Int = 0
}

struct Banner {
    let text: String
}

page Cart requires CartService {
    view {
        text { "cart" }
    }
}

page Promo requires Banner(text: "sale") {
    view {
        text { "promo" }
    }
}
