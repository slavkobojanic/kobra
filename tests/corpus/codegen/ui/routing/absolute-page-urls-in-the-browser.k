// `PublicUrls.absolute(_:)` works in the browser too (12-routing.md,
// "Absolute URLs"): a component injects it like any shared service. The
// browser container's body (the page's typed URL against `location.origin`,
// `$k.pageUrl`) is checked in k_codegen's codegen tests.
import k.server.PublicUrls

page Product at "/product/{id}" {
    @Param let id: Int

    view {
        text { "product {id}" }
    }
}

page Home {
    @Inject let urls: PublicUrls

    @Derived let link: String = "{urls.absolute(.product(id: 7))}"

    view {
        text { link }
    }
}
