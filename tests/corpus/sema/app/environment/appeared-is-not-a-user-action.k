// An element coming into view is not the user acting, so a `@UserAction`
// call from `appeared` is K4030 (45-web-platform.md, "User activation").
import k.device.Clipboard

service Copier {
    @Inject let clipboard: Clipboard

    @State var problem: Bool = false

    func copy() async {
        do {
            try await clipboard.write(text: "seen")
        } catch _ {
            problem = true
        }
    }
}

component AutoCopy {
    @Inject let copier: Copier

    view {
        text { "copy me" }
            .on(appeared: { await copier.copy() })
        button { "Copy" }
            .on(click: { await copier.copy() })
    }
}
