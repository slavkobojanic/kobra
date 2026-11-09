/// The button of `docs/design/45-web-platform.md`'s opening example.
public component UseMyLocation {
    @Inject let location: LocationService
    @Inject let places: PlaceService

    @State var problem: LocateError? = nil

    view {
        if location.canLocate {
            button { "Use my location" }
                .on(click: useLocation)
        }

        if let latitude = places.latitude, let longitude = places.longitude {
            text { "At {latitude}, {longitude}" }
        }

        if let problem {
            text { explain(problem) }
        }
    }

    func explain(_ problem: LocateError) -> String {
        return match problem {
            case .unsupported { "Locating is not supported here" }
            case .denied { "Location access was refused" }
            case .noFix { "No position could be found" }
        }
    }

    func useLocation() async {
        do {
            let position = try await location.locate()
            places.select(latitude: position.latitude, longitude: position.longitude)
        } catch let error {
            problem = error
        }
    }
}
