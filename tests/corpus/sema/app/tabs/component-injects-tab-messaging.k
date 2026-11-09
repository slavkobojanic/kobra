// `TabMessaging` is a client-only platform interface, injectable only into services (K4005), like
// `Geolocation` (45-web-platform.md, "Shapes").
import k.device.TabMessaging

component CartBadge {
    @Inject let tabs: TabMessaging

    view {
        text { "cart" }
    }
}
