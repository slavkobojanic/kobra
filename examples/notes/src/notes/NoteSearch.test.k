import k.crypto.Uuid

/// `k test` runs the worker in-process, through the same codecs
/// (`docs/design/55-workers.md`, "Tests").
test NoteSearch {
    @Inject let search: NoteSearch

    test "a word in the title ranks above one in a tag, and a tag above the text" {
        let inText = note(Uuid("00000000-0000-4000-8000-000000000001"), title: "Groceries", body: "Plan the trip shopping")
        let inTag = note(Uuid("00000000-0000-4000-8000-000000000002"), title: "Packing", tags: ["trip"])
        let inTitle = note(Uuid("00000000-0000-4000-8000-000000000003"), title: "Trip to Lisbon")

        let ids = try await search.rank(notes: [inText, inTag, inTitle], query: "Trip")

        expect ids == [inTitle.id, inTag.id, inText.id]
    }

    test "every word must match" {
        let both = note(Uuid("00000000-0000-4000-8000-000000000001"), title: "Lisbon trip", body: "Book the train")
        let one = note(Uuid("00000000-0000-4000-8000-000000000002"), title: "Porto trip")

        let ids = try await search.rank(notes: [both, one], query: "trip lisbon")

        expect ids == [both.id]
    }

    test "ties keep the order of the notes" {
        let first = note(Uuid("00000000-0000-4000-8000-000000000001"), title: "Ideas")
        let second = note(Uuid("00000000-0000-4000-8000-000000000002"), title: "More ideas")

        let ids = try await search.rank(notes: [first, second], query: "ideas")

        expect ids == [first.id, second.id]
    }

    /// A note with only what the search reads.
    func note(_ id: Uuid, title: String, body: String = "", tags: List<String> = []) -> NoteSummary {
        return NoteSummary(
            id: id,
            title: title,
            body: body,
            tags: tags,
            isPinned: false,
            revision: 1,
            updatedAt: Date(unixSeconds: 1767268800)
        )
    }
}
