public page Home at "/" {
    @Inject let shipping: Shipping

    @State var message: String = ""

    func book() async {
        do {
            message = try await shipping.book(day: 1)
        } catch let e {
            message = "{e}"
        }
    }

    view {
        button { "Book" }
            .on(click: book)
        text { message }
    }
}
