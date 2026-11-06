// Services own state: `@State var` and `@Derived let` are allowed in a service.
service Basket {
    @State var count: Int = 0
    @Derived let isEmpty: Bool = count == 0

    func add() {
        count += 1
    }
}
