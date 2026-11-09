// `Geolocation` is a platform interface, injectable only into services (K4005), like `Clipboard`
// (45-web-platform.md, "Shapes").
import k.device.Geolocation

component LocateButton {
    @Inject let geolocation: Geolocation

    view {
        text { "locate" }
    }
}
