import k.device.CopyButton
import k.device.ShareButton

public page Home at "/" {
    view {
        CopyButton(text: "https://example.com/join/42", label: "Copy invite link")

        ShareButton(content: .page(.home, title: "Invites"), label: "Share this page")
    }
}
