// `Notifications` is a platform interface, injectable only into services (K4005), like `Geolocation`
// (45-web-platform.md, "Shapes").
import k.device.Notifications

component Bell {
    @Inject let notifications: Notifications

    view {
        text { "bell" }
    }
}
