// `guard let x = e else { ... }`
func f() {
    guard let user = session.user else { throw CheckoutError.notLoggedIn }
}
