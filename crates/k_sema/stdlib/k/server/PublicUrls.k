/// Absolute URLs of the app's pages (`docs/design/12-routing.md`,
/// "Absolute URLs"), built from typed `Page` values as `link(to:)` is.
///
/// On the server they start with the public origin `K_PUBLIC_ORIGIN`; in
/// the browser with the page's origin, which is the public origin.
@Native
public service PublicUrls {
    /// Returns the absolute URL of `page` ([`k.core.Page`]), for emails,
    /// push notifications, redirects and links to copy.
    func absolute(_ page: Page) -> Url
    /// Returns the absolute URL of an endpoint page ([`k.core.Endpoint`]).
    /// Server-only, as endpoint pages are.
    func absolute(endpoint: Endpoint) -> Url
}
