/// The draft, from local storage: rendered in the browser only.
@Rendering(.client)
public component Draft {
    @Prop let title: String

    @Inject let drafts: DraftService

    @State var input: String = ""

    view {
        section {
            heading(level: 2) { title }

            paragraph { "Draft: {drafts.text}" }

            textField(value: bind input)
                .label("Draft text")
        }
    }

    loading {
        section {
            heading(level: 2) { title }

            paragraph { "Loading the draft…" }
        }
    }
}
