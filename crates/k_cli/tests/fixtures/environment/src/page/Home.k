/// The fixture's one page.
page Home at "/" {
    view {
        heading(level: 1) { "Environment" }

        OfflineBanner()
        Refresher()
        Motion()
        Layout()
        Feed()
        Measured()
    }
}
