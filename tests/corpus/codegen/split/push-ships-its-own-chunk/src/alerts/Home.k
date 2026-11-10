public page Home at "/" {
    @Inject let alerts: AlertService

    view {
        button { "Notify me" }
            .on(click: { await alerts.subscribe() })
    }
}
