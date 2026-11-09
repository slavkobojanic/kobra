/// How exact a position [`Geolocation`] asks for
/// (`docs/design/45-web-platform.md`, "Geolocation").
public enum LocationAccuracy implements Equatable {
    /// Within a few kilometres: enough for a city's weather or "stores near
    /// me", faster and lighter on the battery. The default.
    case coarse
    /// As exact as the device can tell, from satellites where it has them.
    case precise
}
