/// Client-side navigation (`docs/design/12-routing.md`, "Navigation").
@Native
public class Router {
    /// Navigates to `page`, a page of the app ([`k.core.Page`]), without a
    /// full page load.
    static func navigate(to page: Page)

    /// Leaves the app for `url` with a full page load.
    static func navigate(url: Url)
}
