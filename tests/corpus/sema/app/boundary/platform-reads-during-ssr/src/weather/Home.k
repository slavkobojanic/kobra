public page Home at "/" {
    @Inject let location: LocationService

    view {
        if location.canLocate {
            button { location.label }
        }
        text { location.city }
    }
}
