// `@Load(keepOnReload: true)` marks the load's spec `keep: true`: `reload()` keeps its value while
// its inputs are unchanged (14-async-data.md, "Reloading"). A plain `@Load` has no `keep`.
func lookUp(id: Int) async -> String {
    return "place {id}"
}

func forecastFor(place: String) async -> Int {
    return place.count
}

page City {
    @Param let id: Int

    @Load(keepOnReload: true) let place: String = await lookUp(id: id)
    @Load let forecast: Int = await forecastFor(place: place)

    view {
        text { "{place}: {forecast}" }
    }

    loading {
        text { "Loading…" }
    }
}
