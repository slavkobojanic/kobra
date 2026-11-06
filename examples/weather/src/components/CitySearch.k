import k.time.Clock
import weather.Place
import weather.WeatherService

/// A search field that finds places by name as the user types, and links each
/// result to its forecast.
///
/// The search runs 300 ms after the user stops typing. It is an async
/// `@Effect` that reads `query`, so every keystroke cancels the run still
/// waiting or in flight; no explicit debounce or request bookkeeping is
/// needed, and a slow response can never overwrite a newer one.
///
/// The result is held in a `LoadState` rather than a `@Load` field, because
/// the search starts from user input instead of from the component's inputs,
/// and the idle state (nothing typed yet) is meaningful to show.
public component CitySearch {
    @Inject let weather: WeatherService
    @Inject let clock: Clock

    @State var query: String = ""
    @State var results: LoadState<List<Place>> = .idle

    private let debounceDelay: Duration = 300ms

    @Effect
    func search() async {
        let text = query.trimmed()

        guard text.count >= weather.minimumQueryLength else {
            results = .idle
            return
        }

        await clock.sleep(debounceDelay)

        results = .loading

        do {
            let places = try await weather.searchPlaces(query: text)
            results = .loaded(places)
        } catch let error {
            results = .failed(error)
        }
    }

    view {
        section {
            textField(value: bind query)
                .label("City")
                .placeholder("Search for a city")

            match results {
                case .idle {
                    paragraph { "Type at least two letters to search." }
                        .class(.hint)
                }

                case .loading {
                    paragraph { "Searching…" }
                        .class(.hint)
                }

                case .loaded(let places) where places.isEmpty {
                    paragraph { "No places match “{query.trimmed()}”." }
                        .class(.hint)
                }

                case .loaded(let places) {
                    list {
                        for place in places key place.id {
                            item {
                                link(to: .cityWeather(placeId: place.id)) { place.displayName }
                                    .class(.resultLink)
                            }
                            .class(.result)
                        }
                    }
                    .class(.results)
                }

                case .failed {
                    paragraph { "Search is unavailable right now. Please try again." }
                        .class(.error)
                }
            }
        }
        .class(.search)
    }

    style {
        .search {
            display: grid;
            gap: .sm;
        }

        .hint  { color: .muted; fontSize: .sm; }
        .error { color: .danger; fontSize: .sm; }

        .results {
            listStyle: none;
            margin: 0;
            padding: 0;
            border: 1px solid .border;
            borderRadius: .md;
        }

        .result + .result { borderTop: 1px solid .border; }

        .resultLink {
            display: block;
            padding: .sm .md;
            color: .text;
            textDecoration: none;

            &:hover, &:focus-visible { backgroundColor: .surfaceRaised; }
        }
    }
}
