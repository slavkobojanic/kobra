import layouts.Dashboard

/// A weather dashboard for any city, backed by the free Open-Meteo APIs.
///
/// Pages, services and the theme are found by the compiler from their
/// declarations, so this file only holds what is genuinely app-wide: the
/// default layout and the document entries shared by every page.
///
/// There are no `use … for …` rules. `HttpClient` has exactly one production
/// implementation, the stdlib's `FetchHttpClient`, so it is injected with no
/// rule. Under `k test` the hermetic `TestHttpClient` is the default, so no
/// test can reach Open-Meteo; `WeatherService.test.k` chooses its own
/// `FakeHttpClient` instead.
@Layout(Dashboard)
public app Weather {
    metadata {
        manifest(name: "Weather", shortName: "Weather")
        themeColor(.primary)
        locale(.english)
    }
}
