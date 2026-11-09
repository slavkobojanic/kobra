# Copying and sharing

"Copy this" and "share this" are the **`CopyButton` and `ShareButton` components in `k.device`**. A
view uses them directly, with no service. The design is in `docs/design/45-web-platform.md`.

```k
// examples/kanban/src/pages/CardEditor.k
@Derived let link: String = if let url = ShareContent.page(.boardPage(id: board.value).card("{card.id.value}")).url {
    "{url}"
} else {
    ""
}
// ...
CopyButton(text: link, label: "Copy link to {card.title}")
```

- `CopyButton(text:label:)` renders a button named `label`. A click copies `text` as it is at that
  moment, and the button says "Copied" for two seconds, which screen readers announce too.
- It fires `copied`, and `failed` with a `ClipboardWriteError` (`.denied` or `.unsupported`). When
  nothing listens to `failed`, the button says "Couldn't copy" instead, so the user knows to select the
  text themselves.
- `ShareButton(content:label:)` opens the system share sheet where the browser has one. Where it
  doesn't (most desktop browsers), it copies the link instead and says "Link copied". It fires `shared`,
  `copied`, and `failed` with a `ShareButtonFailure`; closing the sheet fires nothing.
- `ShareContent` is built by `ShareContent.page(_:title:)`, `ShareContent.link(_:title:)` or
  `ShareContent.text(_:title:)`. In the browser, `page` turns the page's URL into an absolute link.

Browsers allow a copy or a share only while the user is acting on the page. The buttons copy and share
in their own click, so there is nothing to get wrong.

## More than a button

To read the clipboard, or to copy or share from your own code, inject the client-only **`Clipboard`**
or **`Share`** interface into a service, as with `Files`. Their members are marked `@UserAction`: call
them from a click, submit or key handler, before the handler's first `await`, or the compiler reports
K4030. `Share.share(_:)` uses up the click, so one click shares once.

```k
// src/notes/NoteSharing.k
import k.device.Share

public service NoteSharing {
    @Inject let share: Share

    @State var problem: String = ""

    /// Hidden where the browser has no share sheet.
    @Derived let canShare: Bool = share.isSupported

    func shareNote(text: String) async {
        do {
            try await share.share(.text(text, title: "A note"))
            problem = ""
        } catch .unsupported {
            problem = "This browser can't share."
        } catch .cancelled {
            problem = ""
        } catch .denied {
            problem = "Sharing was refused."
        }
    }
}
```

## Testing

`k test` puts **`TestClipboard`** and **`TestShare`** in place of the browser's: nothing touches the
real clipboard and no sheet opens. Inject them by type to see what was copied or shared, or to make
the next call fail:

```k
// examples/kanban/src/pages/BoardPage.test.k
screen.button(named: "Edit A").click()
screen.button(named: "Copy link to A").click()
await screen.settle()

expect (clipboard.text ?? "").endsWith("/boards/{id.value}?card={card.id.value}")
expect screen.button(named: "Copy link to A").text == "Copied"
```

`TestClipboard.text` is what the clipboard holds, `set(text:)` copies as another application would,
and `failWrites(error:)`, `failReads(error:)` and `setSupported(_:)` simulate the browser.
`TestShare.shared` lists every share; `cancelNext()`, `failNext(error:)` and `setSupported(_:)`
simulate the user and the browser. After `setSupported(false)`, a `ShareButton` copies to
`TestClipboard`, so a test can check both paths.
