// the error-handling example of the design: payload binding and several patterns per catch
func add() async {
    do {
        try await cart.add(product: product)
    } catch .outOfStock(let available) {
        error = .outOfStock(available: available)
    } catch .productNotFound, .unauthorized {
        Router.navigate(to: .home)
    }
}
