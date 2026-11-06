import k.time.Clock
import k.ui.Icon
import weather.CurrentConditions
import weather.UnitSystem

/// The large "right now" card at the top of the dashboard: temperature,
/// condition, feels-like temperature, humidity and wind.
///
/// The wind arrow is rotated with an `Angle` computed from the wind
/// direction, passed to the style block as a reactive value. `Icon` is a
/// stdlib component with no `@Stylable` hooks, so each icon is wrapped in an
/// element this component styles.
///
/// "Updated 5 minutes ago" is relative to `clock.now`. Until hydration
/// completes, the browser's `Clock` returns the server's render instant, so the
/// server's HTML and the hydrated page show the same text.
public component CurrentConditionsCard {
    @Inject let clock: Clock

    /// The conditions to show.
    @Prop let current: CurrentConditions

    /// The unit system `current` is expressed in.
    @Prop let units: UnitSystem

    /// When the conditions were last updated by Open-Meteo.
    @Prop let updatedAt: Date

    /// The arrow points where the wind blows to, which is opposite to where it
    /// comes from.
    @Derived let windArrowAngle: Angle = Angle(degrees: current.windDirection) + 180deg

    view {
        article {
            heading(level: 2) { "Now" }
                .class(.title)

            box {
                box { Icon(name: current.condition.icon(isDay: current.isDay)) }
                    .class(.icon)

                text { units.temperature(value: current.temperature) }
                    .class(.temperature)
            }
            .class(.headline)

            paragraph { current.condition.label }
                .class(.condition)

            definitionList {
                term { "Feels like" }

                definition { units.temperature(value: current.feelsLike) }

                term { "Humidity" }

                definition { "{current.humidity}%" }

                term { "Wind" }

                definition {
                    text { Icon(name: .arrowUp) }
                        .class(.windArrow)

                    text { units.windSpeed(value: current.windSpeed) }
                }
            }
            .class(.details)

            paragraph { "Updated {updatedAt.formatted(as: .relative, from: clock.now)}" }
                .class(.updated)
        }
        .class(.card)
    }

    style {
        .card {
            display: grid;
            gap: .sm;
            padding: .lg;
            borderRadius: .lg;
            backgroundColor: .surfaceRaised;
        }

        .title       { fontSize: .sm; color: .muted; textTransform: uppercase; }
        .headline    { display: flex; alignItems: center; gap: .md; }
        .icon        { width: 3rem; height: 3rem; }
        .temperature { fontSize: .display; fontWeight: 600; lineHeight: 1; }
        .condition   { fontSize: .lg; }

        .details {
            display: grid;
            gridTemplateColumns: auto 1fr;
            gap: .xs .md;
            margin: 0;
        }

        .windArrow {
            display: inline-block;
            width: 1em;
            height: 1em;
            marginRight: .xs;
            rotate: {windArrowAngle};
        }

        .updated { fontSize: .sm; color: .muted; }
    }
}
