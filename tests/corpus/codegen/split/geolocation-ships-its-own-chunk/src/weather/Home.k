public page Home at "/" {
    @Inject let location: LocationService

    view {
        if location.canLocate {
            button { "Use my location" }
                .on(click: { await location.locate() })
        }
        text { "{location.latitude}" }
    }
}
