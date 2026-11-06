import layouts.Dashboard

/// A weather dashboard for any city, backed by the free Open-Meteo APIs.
///
/// Pages, services and the theme are found by the compiler from their
/// declarations, so this file only holds what is genuinely app-wide: the
/// default layout and the document entries shared by every page.
///
/// There are no `use … for …` rules. `HttpClient` has exactly one
/// implementation outside tests, the stdlib's fetch-based client, so it is
/// injected automatically. Tests replace it per test group.
@Layout(Dashboard)
public app Weather {
    metadata {
        manifest(name: "Weather", shortName: "Weather")
        themeColor(.primary)
        locale(.english)
    }
}
