import k.device.Geolocation

// Client-only: it injects `Geolocation`.
public service LocationService {
    @Inject let geolocation: Geolocation

    // Hidden when the browser can't locate, or the user has blocked it.
    @Derived let canLocate: Bool = geolocation.isSupported and geolocation.permission != .denied

    // Built on the one above, so it has a server value too.
    @Derived let label: String = if canLocate { "Use my location" } else { "" }

    @State var city: String = ""
}
