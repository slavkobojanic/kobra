// A member declared `@UserAction(consuming: true)` uses up the user's action, as a share sheet does,
// so a second consuming call needs another click (45-web-platform.md, "Consuming calls"). Two on one
// path, directly or through a function that makes one, are K4030 at the second; one in each branch
// of an `if` is fine; one in a loop body is a second on the next iteration.
import k.core.UserAction

interface Sharing {
    @UserAction(consuming: true)
    func share(text: String) async

    @UserAction
    func prepare()
}

service Sharer implements Sharing {
    @State var shared: List<String> = []

    func share(text: String) async {
        shared.append(text)
    }

    func prepare() {
        shared = []
    }
}

service Links {
    @Inject let sharing: Sharing

    func shareLink(_ url: String) async {
        sharing.prepare()
        await sharing.share(text: url)
    }
}

component ShareBar {
    @Inject let sharing: Sharing
    @Inject let links: Links

    @Prop let urls: List<String>

    func shareOne(short: Bool) async {
        if short {
            await sharing.share(text: "short")
        } else {
            await sharing.share(text: "long")
        }
    }

    view {
        button { "Share" }
            .on(click: { await shareOne(short: true) })
        button { "Share both" }
            .on(click: {
                sharing.prepare()
                await sharing.share(text: "a")
                await links.shareLink("b")
            })
        button { "Share all" }
            .on(click: {
                for url in urls {
                    await sharing.share(text: url)
                }
            })
    }
}
