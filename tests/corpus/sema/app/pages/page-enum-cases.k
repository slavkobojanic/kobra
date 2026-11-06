// Every view page is a case of the generated `Page` enum, with its path parameters as payload.
page Pricing {
    view {
        text { "pricing" }
    }
}

page Product {
    @Param let id: Int

    view {
        link(to: .pricing) { "Pricing" }
        //       ^^^^^^^^ type: Page
        //        ^^^^^^^ resolves: member Page.pricing
        link(to: .product(id: id)) { "Again" }
        //       ^^^^^^^^^^^^^^^^ type: Page
        //        ^^^^^^^ resolves: member Page.product
    }
}
