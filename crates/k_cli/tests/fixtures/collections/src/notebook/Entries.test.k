import k.storage.Collection
import k.storage.CollectionIndex
import k.storage.StorageEstimate
import k.test.MemoryCollections
import k.test.TestClock

// Test code may declare an index of the app's collection, to query what a
// service stored by a field the app doesn't index.
let byTopic: CollectionIndex<String, Entry> = CollectionIndex(collection: "entries", field: "topic")

test Entries {
    @Inject let collections: MemoryCollections
    @Inject let clock: TestClock
    @Inject let entries: Entries

    test "nothing is stored at first" {
        await entries.refresh()

        expect entries.count == 0
        expect entries.newest.isEmpty
        expect try await entries.entry(1) == nil
    }

    test "a written entry is read back by its key" {
        await entries.write("first", topic: nil)

        let first = try await entries.entry(1)
        expect first?.text == "first"
        expect entries.count == 1
    }

    test "the newest come first, at most three" {
        for text in ["a", "b", "c", "d"] {
            await entries.write(text, topic: nil)
            clock.advance(by: 1.minutes)
        }

        expect entries.newest.map { e => e.text } == ["d", "c", "b"]
        expect entries.count == 4
    }

    test "dates order chronologically, also across milliseconds" {
        let start = Date(unixSeconds: 1793966400)
        try await entries.writeAll([
            Entry(id: 1, text: "later", writtenAt: start + 1.seconds, topic: nil),
            Entry(id: 2, text: "between", writtenAt: start + 500.milliseconds, topic: nil),
            Entry(id: 3, text: "first", writtenAt: start, topic: nil),
        ])

        let list = try await entries.between(start, start + 1.seconds)
        expect list.map { e => e.text } == ["first", "between", "later"]
        let some = try await entries.between(start + 1.milliseconds, start + 999.milliseconds)
        expect some.map { e => e.text } == ["between"]
    }

    test "all is in key order, numbers numerically" {
        let at = Date(unixSeconds: 1793966400)
        try await entries.writeAll([
            Entry(id: 10, text: "ten", writtenAt: at, topic: nil),
            Entry(id: 9, text: "nine", writtenAt: at, topic: nil),
            Entry(id: 2, text: "two", writtenAt: at, topic: nil),
        ])

        let all = try await entries.all()
        expect all.map { e => e.id } == [2, 9, 10]
    }

    test "putting a value with the same key replaces it" {
        let at = Date(unixSeconds: 1793966400)
        try await entries.writeAll([Entry(id: 1, text: "old", writtenAt: at, topic: nil)])
        try await entries.writeAll([Entry(id: 1, text: "new", writtenAt: at, topic: nil)])

        let all = try await entries.all()
        expect all.map { e => e.text } == ["new"]
    }

    test "deleting removes one, and deleting a missing key succeeds" {
        await entries.write("a", topic: nil)
        await entries.write("b", topic: nil)

        try await entries.erase(1)
        try await entries.erase(42)

        let all = try await entries.all()
        expect all.map { e => e.text } == ["b"]
    }

    test "clearing empties the collection" {
        await entries.write("a", topic: nil)

        await entries.eraseAll()

        expect entries.count == 0
    }

    test "a stored value that doesn't decode reads as unreadable" {
        collections.writeRaw(collection: "entries", text: "{{\"id\": 7, \"text\": 3, \"writtenAt\": \"2026-11-10T00:00:00Z\"}")

        await entries.refresh()

        expect entries.problem == "unreadable"
        expect throws StorageReadError.unreadable {
            try await entries.entry(7)
        }
    }

    test "a value an older version stored without the optional topic still reads" {
        collections.writeRaw(collection: "entries", text: "{{\"id\": 7, \"text\": \"old\", \"writtenAt\": \"2026-11-10T00:00:00Z\"}")

        let old = try await entries.entry(7)
        expect old?.text == "old"
        expect old?.topic == nil
    }

    test "failing reads and writes are reported, and stop" {
        collections.failReads(error: .unavailable)
        await entries.refresh()
        expect entries.problem == "unavailable"

        collections.stopFailing()
        collections.failWrites(error: .quotaExceeded)
        await entries.write("too much", topic: nil)
        expect entries.problem == "full"

        collections.stopFailing()
        await entries.write("fits", topic: nil)
        expect entries.problem == ""
        expect entries.count == 1
    }

    test "persistence is best effort until set" {
        expect await entries.keep() == .bestEffort

        collections.setPersistence(.persistent)

        expect await entries.keep() == .persistent
        expect await collections.persistence() == .persistent
    }

    test "the estimate is what the test sets" {
        expect await collections.estimate() == nil

        collections.setEstimate(StorageEstimate(usage: 1024, quota: 1048576))

        expect await collections.estimate() == StorageEstimate(usage: 1024, quota: 1048576)
    }

    test "a test may redeclare the collection to inspect it" {
        let stored: Collection<Int, Entry> = Collection(name: "entries", key: "id")
        await entries.write("seen", topic: nil)

        let all = try await collections.all(in: stored)
        expect all.map { e => e.text } == ["seen"]
        expect stored.name == "entries"
    }

    test "an index finds by its field, leaving out values whose field is nil" {
        await entries.write("#one", topic: "tagged")
        await entries.write("two", topic: nil)
        await entries.write("#three", topic: "tagged")

        let tagged = try await collections.find("tagged", in: byTopic)
        expect tagged.map { e => e.text } == ["#one", "#three"]
        let all = try await collections.range(in: byTopic)
        expect all.count == 2
        expect byTopic.field == "topic"
        expect byTopic.collection == "entries"
    }
}
