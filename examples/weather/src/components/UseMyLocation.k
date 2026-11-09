import weather.LocateError
import weather.LocationService

/// "Use my location": opens the weather at the device's position.
///
/// The server renders nothing here (it can't locate), and the button
/// appears after hydration in browsers that can, unless the user has
/// blocked location for the site. The first click shows the browser's
/// prompt; a refusal hides the button and says so.
public component UseMyLocation {
    @Inject let location: LocationService

    @State var problem: LocateError? = nil

    view {
        if location.canLocate {
            button { "Use my location" }
                .on(click: useLocation)
                .class(.locate)
        }

        if let problem {
            paragraph { explain(problem: problem) }
                .class(.problem)
        }
    }

    /// Locates the device, then opens its weather. A click is a user's
    /// action, so `locate()` may prompt from here.
    func useLocation() async {
        problem = nil

        do {
            let position = try await location.locate()
            Router.navigate(to: .localWeather(latitude: "{position.latitude}", longitude: "{position.longitude}"))
        } catch let error {
            problem = error
        }
    }

    /// A sentence explaining why there is no position.
    private func explain(problem: LocateError) -> String {
        return match problem {
            case .unsupported { "This browser can't tell where you are." }

            case .denied { "Location access is off for this site. Search for a city instead." }

            case .noFix { "Your location couldn't be found. Search for a city instead." }
        }
    }

    style {
        .locate {
            justifySelf: start;
            padding: .sm .md;
            borderRadius: .md;
        }

        .problem { color: .muted; }
    }
}
