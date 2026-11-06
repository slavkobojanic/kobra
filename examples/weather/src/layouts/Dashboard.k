/// The shell around every page: a header linking home, the page content, and
/// the attribution that Open-Meteo's CC BY 4.0 licence requires.
///
/// The `Weather` app applies this layout to every page, so pages don't repeat
/// `@Layout(Dashboard)`. Its `metadata` entries are defaults that each page
/// overrides, and the only title a failed `CityWeather` load has unless the
/// failure sets its own.
public layout Dashboard {
    /// The current page.
    @Slot let content: View

    metadata {
        title("Weather")
        description("Current weather, air quality and hourly temperature for any city.")
    }

    view {
        header {
            link(to: .home) { "Weather" }
                .class(.brand)
        }
        .class(.header)

        main { content }
            .class(.main)

        footer {
            text { "Weather data by " }

            link(url: "https://open-meteo.com") { "Open-Meteo.com" }

            text { ", licensed under CC BY 4.0." }
        }
        .class(.footer)
    }

    style {
        .header {
            padding: .md .lg;
            borderBottom: 1px solid .border;
        }

        .brand {
            fontSize: .lg;
            fontWeight: 700;
            color: .text;
            textDecoration: none;
        }

        .main {
            display: grid;
            gap: .lg;
            maxWidth: 72rem;
            margin: 0 auto;
            padding: .lg;
        }

        .footer {
            padding: .lg;
            fontSize: .sm;
            color: .muted;
            textAlign: center;
        }
    }
}
