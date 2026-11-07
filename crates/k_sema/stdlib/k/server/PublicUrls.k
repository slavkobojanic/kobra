/// Absolute URLs from the public origin `K_PUBLIC_ORIGIN`
/// (`docs/design/12-routing.md`, "Absolute URLs"). Server-only.
@ServerOnly
@Native
public service PublicUrls {
    /// Returns the absolute URL of `page` ([`k.core.Page`]), for emails and
    /// redirects to the app.
    func absolute(_ page: Page) -> Url
    /// Returns the absolute URL of an endpoint page ([`k.core.Endpoint`]).
    func absolute(endpoint: Endpoint) -> Url
}
