import k.device.AppUpdates

service Releases {
    @Inject let updates: AppUpdates

    @Derived let label: String = if updates.isAvailable { "update" } else { "current" }
}
