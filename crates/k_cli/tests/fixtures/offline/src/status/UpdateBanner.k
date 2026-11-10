import k.device.AppUpdates

/// Offers a newer build once its service worker has taken over.
public component UpdateBanner {
    @Inject let updates: AppUpdates

    view {
        if updates.isAvailable {
            paragraph { "A new version is ready." }

            button { "Reload" }
                .on(click: updates.apply)
        }
    }
}
