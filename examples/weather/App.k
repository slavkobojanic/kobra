import layouts.Dashboard

/// A weather dashboard for any city, backed by the free Open-Meteo APIs.
///
/// Pages, services and the theme are found by the compiler from their
/// declarations, so this file only holds what is genuinely app-wide: the
/// default layout and the document entries shared by every page.
///
/// `@Offline` keeps the last forecast (docs/design/54-offline.md): `k build`
/// adds a service worker that caches the build's files and every page the
/// browser loads through it, so a place viewed before opens without a
/// network, with the forecast as it was then ("Updated …" says when).
///
/// There are no `use … for …` rules. `k.http.HttpClient` has exactly one
/// production implementation, the stdlib's `k.http.FetchHttpClient`, so it is
/// injected with no rule. Under `k test` the hermetic `k.test.TestHttpClient`
/// is the default, so no test can reach Open-Meteo; the tests in
/// `weather/WeatherService.test.k` choose their own `FakeHttpClient` instead.
/// `k.time.Clock` is a service with no rule either, and `k test` replaces it
/// with `k.test.TestClock`.
@Layout(Dashboard)
@Offline
public app Weather {
    metadata {
        manifest(name: "Weather", shortName: "Weather")
        themeColor(.primary)
        locale(.english)
    }
}
