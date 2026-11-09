import k.device.CopyButton

/// A copy button whose failure the panel reports itself.
component ListeningPanel {
    @State var problem: String = ""
    @State var copies: Int = 0

    view {
        section {
            CopyButton(text: "secret code", label: "Copy code")
                .on(copied: { copies = copies + 1 })
                .on(failed: { error =>
                    problem = if error == .denied { "Copying was refused" } else { "No clipboard here" }
                })

            paragraph { "Copied {copies} times" }

            if problem.isNotEmpty {
                paragraph { problem }
            }
        }
    }
}
