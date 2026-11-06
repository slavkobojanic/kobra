// A cycle through an interface's only implementation is still a cycle.
interface Pricing {
    func price(sku: String) -> Int
}

service Catalog {
    @Inject let pricing: Pricing
}

service TablePricing implements Pricing {
    @Inject let catalog: Catalog

    func price(sku: String) -> Int {
        return 1
    }
}
