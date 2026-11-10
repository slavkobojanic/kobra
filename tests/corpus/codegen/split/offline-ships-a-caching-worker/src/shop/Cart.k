import k.device.AppUpdates

page Cart {
    @Inject let updates: AppUpdates

    view {
        if updates.isAvailable {
            button { "Reload" }
                .on(click: updates.apply)
        }
    }
}
