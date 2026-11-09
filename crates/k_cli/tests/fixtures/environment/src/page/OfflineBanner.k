import k.device.Connectivity

/// Says so while the device is offline.
public component OfflineBanner {
    @Inject let connectivity: Connectivity

    view {
        if not connectivity.isOnline {
            paragraph { "You're offline" }
                .role(.status)
        }
    }
}
