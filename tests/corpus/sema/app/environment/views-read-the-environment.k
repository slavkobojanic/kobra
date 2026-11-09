// The environment services are stdlib services usable on both sides
// (45-web-platform.md, "Shapes"): components, pages, layouts and services
// inject them, and views read their members, which the server answers too.
import k.device.Connectivity
import k.device.PageVisibility
import k.device.PointerKind
import k.device.Preferences
import k.device.Viewport
import k.device.ViewportSize

component Banner {
    @Inject let connectivity: Connectivity
    @Inject let viewport: Viewport
    @Inject let preferences: Preferences

    view {
        if not connectivity.isOnline {
            text { "You're offline" }
        }
        if viewport.pointer == PointerKind.coarse and viewport.canHover == false {
            text { "Tap" }
        }
        if let size = viewport.size {
            text { "Known" }
        }
        if preferences.prefersDark or preferences.prefersReducedMotion {
            text { "Calm" }
        }
    }
}

service SyncService {
    @Inject let connectivity: Connectivity
    @Inject let visibility: PageVisibility

    @Derived let canSync: Bool = connectivity.isOnline and visibility.isVisible
}

page Home at "/" {
    @Inject let sync: SyncService
    @Inject let viewport: Viewport

    @State var phone: Bool = false

    @Effect
    func follow() {
        phone = viewport.size == ViewportSize(width: 390px, height: 844px)
    }

    view {
        Banner()
        text { "{sync.canSync}" }
    }
}
