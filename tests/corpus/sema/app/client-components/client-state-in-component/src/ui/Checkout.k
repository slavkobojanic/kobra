// `CartTotal` is rendered here on the server as well as inside `CartPanel`.
public page Checkout {
    view {
        CartBadge()
        CartPanel()
        CartTotal()
    }
}
