public page Home at "/" {
    @Inject let alerts: AlertService

    view {
        if alerts.canNotify {
            button { "Notify me" }
                .on(click: { await alerts.enable() })
        }
    }
}
