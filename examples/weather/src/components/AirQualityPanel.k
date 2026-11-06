import weather.AirQuality

/// The current European Air Quality Index as a colored badge, with
/// particulate levels when Open-Meteo models them for the place.
public component AirQualityPanel {
    /// The air quality to show.
    @Prop let airQuality: AirQuality

    /// Good and fair air is shown in the positive color, moderate air as a
    /// caution, and anything worse as a danger.
    @Derived let badgeColor: Color = match airQuality.category {
        case .good, .fair { .positive }

        case .moderate { .caution }

        case .poor, .veryPoor, .extremelyPoor { .danger }

        case .unavailable { .muted }
    }

    view {
        section {
            heading(level: 2) { "Air quality" }

            text { airQuality.category.label }
                .class(.badge)

            if let index = airQuality.europeanIndex {
                paragraph { "European AQI {index}" }
                    .class(.detail)
            }

            if let pm25 = airQuality.pm25 {
                paragraph { "PM2.5 {pm25.formatted(as: .wholeNumber)} µg/m³" }
                    .class(.detail)
            }

            if let pm10 = airQuality.pm10 {
                paragraph { "PM10 {pm10.formatted(as: .wholeNumber)} µg/m³" }
                    .class(.detail)
            }
        }
        .class(.panel)
    }

    style {
        .panel {
            display: grid;
            gap: .xs;
            alignContent: start;
            padding: .lg;
            borderRadius: .lg;
            backgroundColor: .surfaceRaised;
        }

        .badge {
            justifySelf: start;
            padding: .xs .sm;
            borderRadius: .md;
            color: .surface;
            backgroundColor: {badgeColor};
            fontWeight: 600;
        }

        .detail { fontSize: .sm; color: .muted; }
    }
}
