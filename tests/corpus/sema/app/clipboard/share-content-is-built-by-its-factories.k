// `ShareContent` has no memberwise initializer: it is built by `page`, `link` or `text`, so it
// always carries something to share (45-web-platform.md, "Web Share").
import k.device.ShareContent

func empty() -> ShareContent {
    return ShareContent()
}

func link() -> ShareContent {
    return .link(Url("https://example.com"), title: "Example")
}

func words() -> ShareContent {
    return ShareContent.text("hello")
}
