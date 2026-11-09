// A hydrated load group runs nothing, so each load lists its reactive reads
// without the request in `inputs`: a change of `units` after hydration
// re-runs `forecast` alone, never `place` (runtime.md §7; weather-E1). A
// load that reads only other loads has no `inputs`. An effect that waits for
// the loads reads `ready()`, which a reload's new values don't change, so
// an `@Effect(on: [placeId])` timer isn't recreated on every reload
// (weather-E2).
struct Place implements Codable {
    let name: String
}

enum Units implements Codable {
    case metric
    case imperial
}

service Weather {
    func place(id: Int) async -> Place {
        return Place(name: "Berlin")
    }

    func forecast(place: Place, units: Units) async -> String {
        return "sunny"
    }

    func air(place: Place) async -> String {
        return "good"
    }
}

page CityWeather {
    @Param let placeId: Int
    @Param let units: Units = .metric
    @Inject let weather: Weather

    @Load let place: Place = await weather.place(id: placeId)
    @Load let forecast: String = await weather.forecast(place: place, units: units)
    @Load let air: String = await weather.air(place: place)

    @State var seen: String = ""

    @Effect(on: [placeId])
    func refresh() {
        seen = place.name
    }

    view {
        text { "{place.name}: {forecast}, {air}" }
    }

    loading {
        text { "Loading…" }
    }
}
