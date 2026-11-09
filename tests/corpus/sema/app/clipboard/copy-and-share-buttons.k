// `CopyButton` and `ShareButton` need no service (45-web-platform.md, "Shapes"): a view uses them
// directly, listening to their events if it wants to.
import k.device.ClipboardWriteError
import k.device.CopyButton
import k.device.ShareButton
import k.device.ShareButtonFailure

page Home at "/" {
    @State var problem: String = ""

    view {
        CopyButton(text: "https://example.com/join/42", label: "Copy invite link")
            .on(failed: { error => problem = if error == ClipboardWriteError.denied { "refused" } else { "none" } })

        ShareButton(content: .page(.home, title: "Home"), label: "Share this page")
            .on(failed: { failure => problem = if failure == ShareButtonFailure.denied { "refused" } else { "none" } })
            .on(copied: { problem = "" })

        CopyButton(text: problem, label: "Copy the problem")
    }
}
