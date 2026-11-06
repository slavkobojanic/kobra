// Two services sharing a dependency form a diamond, not a cycle.
service Prices {
    @State var cents: Int = 0
}

service Cart {
    @Inject let prices: Prices
}

service Wishlist {
    @Inject let prices: Prices
}

service Shop {
    @Inject let cart: Cart
    @Inject let wishlist: Wishlist
}
