import k.device.Connectivity
import k.device.Preferences

public page Home at "/" {
    @Inject let connectivity: Connectivity
    @Inject let preferences: Preferences

    view {
        if not connectivity.isOnline {
            text { "You're offline" }
        }
        if preferences.prefersDark {
            text { "Dark" }
        }
    }
}
