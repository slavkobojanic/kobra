// `throw` with a qualified case, an implicit case and a call
func f() {
    throw CheckoutError.notLoggedIn
    throw .outOfStock(available: 0)
    throw makeError()
}
