import k.device.AppUpdates

component UpdateBanner {
    @Inject let updates: AppUpdates

    view {
        if updates.isAvailable {
            button { "Reload" }
                .on(click: updates.apply)
        }
    }
}
