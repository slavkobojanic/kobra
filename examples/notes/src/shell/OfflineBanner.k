import k.device.Connectivity

/// Says so while the device is offline, above every page.
///
/// `Connectivity.isOnline` is `false` when the browser has no network, and
/// also when a call to the server just failed for want of one, so the banner
/// and the alerts of the pages agree. It is `true` during server rendering,
/// so the banner only ever appears in the browser.
public component OfflineBanner {
    @Inject let connectivity: Connectivity

    view {
        if not connectivity.isOnline {
            paragraph { "You're offline. K Notes can't save changes until the connection is back." }
                .class(.banner)
                .role(.status)
        }
    }

    style {
        .banner { margin: 0; padding: .sm .lg; backgroundColor: .surfaceRaised; borderBottom: 1px solid .border; color: .foreground; }
    }
}
