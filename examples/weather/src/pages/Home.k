import components.CitySearch

/// The start page: search for any place, or jump to one of a few featured
/// cities.
///
/// `Home` has a named URL because it is the site root, which a default URL
/// derived from the page name (`/home`) can't express.
public page Home at "/" {
    let featuredCities: List<FeaturedCity> = [
        FeaturedCity(placeId: 2950159, name: "Berlin"),
        FeaturedCity(placeId: 2643743, name: "London"),
        FeaturedCity(placeId: 5128581, name: "New York"),
        FeaturedCity(placeId: 1850147, name: "Tokyo"),
        FeaturedCity(placeId: 2147714, name: "Sydney"),
        FeaturedCity(placeId: 3448439, name: "São Paulo")
    ]

    metadata {
        title("Weather")
        description("Search for any city to see its current weather, air quality and hourly temperature.")
    }

    view {
        section {
            heading(level: 1) { "Weather" }

            paragraph { "Search for a city to see its forecast." }
                .class(.lead)

            CitySearch()
        }
        .class(.intro)

        section {
            heading(level: 2) { "Featured cities" }

            list {
                for city in featuredCities key city.placeId {
                    item {
                        link(to: .cityWeather(placeId: city.placeId)) { city.name }
                            .class(.city)
                    }
                }
            }
            .class(.cities)
        }
    }

    style {
        .intro {
            display: grid;
            gap: .md;
            maxWidth: 32rem;
        }

        .lead { fontSize: .lg; color: .muted; }

        .cities {
            display: flex;
            flexWrap: wrap;
            gap: .sm;
            listStyle: none;
            padding: 0;
        }

        .city {
            display: inline-block;
            padding: .sm .md;
            borderRadius: .md;
            backgroundColor: .surfaceRaised;
            color: .text;
            textDecoration: none;

            &:hover, &:focus-visible { color: .primary; }
        }
    }
}

/// A city linked from the start page. The GeoNames id is enough to open its
/// dashboard; the name is shown only so the link has a label.
struct FeaturedCity {
    let placeId: Int
    let name: String
}
