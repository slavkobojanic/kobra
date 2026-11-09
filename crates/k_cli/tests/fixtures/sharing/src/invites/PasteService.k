import k.device.Clipboard
import k.device.Share
import k.device.ShareError

/// Reads and writes the clipboard, and shares, for the panels.
public service PasteService {
    @Inject let clipboard: Clipboard
    @Inject let share: Share

    @State var pasted: String = ""

    /// Whether the panels may offer a paste button.
    @Derived let canPaste: Bool = clipboard.isSupported

    /// Copies `text`, reporting a failure as a message.
    func copy(_ text: String) async -> String {
        do {
            try await clipboard.write(text: text)
            return "copied"
        } catch .unsupported {
            return "unsupported"
        } catch .denied {
            return "denied"
        }
    }

    /// Pastes the clipboard's text into `pasted`.
    func paste() async {
        do {
            pasted = try await clipboard.readText() ?? "(no text)"
        } catch .unsupported {
            pasted = "(unsupported)"
        } catch .denied {
            pasted = "(denied)"
        }
    }

    /// Shares a page of the app.
    func sharePage(title: String) async -> String {
        do {
            try await share.share(.page(.home, title: title))
            return "shared"
        } catch .unsupported {
            return "unsupported"
        } catch .cancelled {
            return "cancelled"
        } catch .denied {
            return "denied"
        }
    }

    /// Shares a piece of text.
    func shareText(_ text: String) async throws ShareError {
        try await share.share(.text(text))
    }
}
