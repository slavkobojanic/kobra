/// "Notify me when it's ready": asks for permission from the click, then
/// announces the export.
public component ExportButton {
    @Inject let exports: ExportService

    @State var said: String = ""

    view {
        if exports.canNotify {
            button { "Notify me when it's ready" }
                .on(click: notify)
        }

        if said != "" {
            text { said }
        }
    }

    func notify() async {
        if await exports.enable() {
            said = await exports.tryAnnounce(id: "42")
        } else {
            said = "No notifications"
        }
    }
}
