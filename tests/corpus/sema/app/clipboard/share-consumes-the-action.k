// `Share.share(_:)` consumes the user's action (45-web-platform.md, "Web Share"): a second share in
// one click is K4030.
import k.device.Share

service Sharer {
    @Inject let share: Share

    func shareBoth() async {
        do {
            try await share.share(.text("one"))
            try await share.share(.link(Url("https://example.com/two")))
        } catch _ {
        }
    }

    func shareOne() async {
        do {
            try await share.share(.page(.home, title: "Home"))
        } catch _ {
        }
    }
}

page Home at "/" {
    @Inject let sharer: Sharer

    view {
        button { "Share" }
            .on(click: { await sharer.shareOne() })
        button { "Share both" }
            .on(click: { await sharer.shareBoth() })
    }
}
