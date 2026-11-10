import k.crypto.Uuid
import k.test.MemoryCollections
import k.test.TestClock

test OfflineEdits {
    @Inject let offline: OfflineEdits
    @Inject let collections: MemoryCollections
    @Inject let clock: TestClock

    test "a kept edit is read back by its note, and a newer one replaces it" {
        let note = Uuid("1b4e28ba-2fa1-41d2-883f-0016d3cca427")

        expect await offline.keep(note: note, draft: draft(title: "Plan"))
        expect await offline.keep(note: note, draft: draft(title: "Plan B"))

        let edit = await offline.edit(of: note)
        expect edit?.title == "Plan B"
        expect edit?.draft() == draft(title: "Plan B")
    }

    test "the newest edits come first, five at most" {
        for n in 1...6 {
            await offline.keep(note: Uuid.random(), draft: draft(title: "Note {n}"))
            clock.advance(by: 1.minutes)
        }

        await offline.loadNewest()

        expect offline.newest.map { e => e.title } == ["Note 6", "Note 5", "Note 4", "Note 3", "Note 2"]
    }

    test "discarding forgets the edit, also in the list" {
        let note = Uuid("1b4e28ba-2fa1-41d2-883f-0016d3cca427")
        await offline.keep(note: note, draft: draft(title: "Plan"))
        await offline.loadNewest()

        await offline.discard(note: note)

        expect await offline.edit(of: note) == nil
        expect offline.newest.isEmpty
    }

    test "when this browser can't keep the edit either, it says so" {
        collections.failWrites(error: .quotaExceeded)

        let kept = await offline.keep(note: Uuid("1b4e28ba-2fa1-41d2-883f-0016d3cca427"), draft: draft(title: "Plan"))
        expect not kept
    }

    test "an edit an older version kept in another shape counts as none" {
        collections.writeRaw(collection: "unsavedEdits", text: "{{\"noteId\": \"1b4e28ba-2fa1-41d2-883f-0016d3cca427\", \"heading\": \"Plan\"}")

        expect await offline.edit(of: Uuid("1b4e28ba-2fa1-41d2-883f-0016d3cca427")) == nil
    }
}
