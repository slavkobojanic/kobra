/// Absolute URLs from the public origin `K_PUBLIC_ORIGIN`
/// (`docs/design/12-routing.md`, "Absolute URLs"). Server-only.
@ServerOnly
@Native
public service PublicUrls {
    /// The absolute URL of a page.
    func absolute(_ page: Page) -> Url
    /// The absolute URL of an endpoint page.
    func absolute(endpoint: Endpoint) -> Url
}
