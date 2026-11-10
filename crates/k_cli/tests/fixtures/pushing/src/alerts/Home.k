/// The fixture's start page.
page Home at "/" {
    view {
        heading(level: 1) { "Alerts" }

        AlertSwitch()
    }
}
