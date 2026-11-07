/// A page with a load and its own metadata, neither of which the server
/// renders under client rendering.
public page Board {
    @Inject let notes: Notes

    @Load let note: Note = await notes.first()

    metadata {
        title("Board page title")
    }

    view {
        paragraph { note.text }
    }

    loading {
        paragraph { "board loading" }
    }
}
