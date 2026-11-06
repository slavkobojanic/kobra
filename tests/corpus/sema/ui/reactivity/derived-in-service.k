// Services may derive from their own state.
service Basket {
    @State var prices: List<Int> = []
    @Derived let isEmpty: Bool = prices.isEmpty
}
