import checkout.Checkout

/// The start page.
public page Home at "/" {
    @Inject let checkout: Checkout

    view {
        text { "Shop" }
    }
}
