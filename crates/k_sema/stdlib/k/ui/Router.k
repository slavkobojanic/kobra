/// Client-side navigation (`docs/design/12-routing.md`, "Navigation").
@Native
public class Router {
    /// Navigates to a page of the app.
    static func navigate(to page: Page)

    /// Leaves the app for `url` with a full page load.
    static func navigate(url: Url)
}
