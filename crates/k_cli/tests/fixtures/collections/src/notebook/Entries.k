import k.storage.Collection
import k.storage.CollectionIndex
import k.storage.Collections
import k.storage.StoragePersistence
import k.storage.StorageReadError
import k.storage.StorageWriteError
import k.time.Clock

/// The notebook's entries, in the browser's IndexedDB.
public service Entries {
    @Inject let collections: Collections
    @Inject let clock: Clock

    private let entries: Collection<Int, Entry> = Collection(name: "entries", key: "id")
    private let byTime: CollectionIndex<Date, Entry> = CollectionIndex(collection: "entries", field: "writtenAt")

    /// The newest entries, newest first.
    @State var newest: List<Entry> = []

    /// How many entries there are.
    @State var count: Int = 0

    /// What went wrong last, or `""`.
    @State var problem: String = ""

    /// Reads the newest three entries and the count again.
    func refresh() async {
        do {
            newest = try await collections.range(in: byTime, reversed: true, limit: 3)
            count = try await collections.count(in: entries)
            problem = ""
        } catch .unavailable {
            problem = "unavailable"
        } catch .unreadable {
            problem = "unreadable"
        }
    }

    /// Writes an entry numbered after the others.
    func write(_ text: String, topic: String?) async {
        let entry = Entry(id: count + 1, text: text, writtenAt: clock.now, topic: topic)
        do {
            try await collections.put(entry, into: entries)
        } catch .unavailable {
            problem = "unavailable"
            return
        } catch .quotaExceeded {
            problem = "full"
            return
        }
        await refresh()
    }

    /// The entry numbered `id`, or `nil`.
    func entry(_ id: Int) async throws StorageReadError -> Entry? {
        return try await collections.get(id, from: entries)
    }

    /// Every entry, in key order.
    func all() async throws StorageReadError -> List<Entry> {
        return try await collections.all(in: entries)
    }

    /// The entries written between `from` and `to`, oldest first.
    func between(_ from: Date, _ to: Date) async throws StorageReadError -> List<Entry> {
        return try await collections.range(in: byTime, from: from, to: to)
    }

    /// Writes several entries at once.
    func writeAll(_ list: List<Entry>) async throws StorageWriteError {
        try await collections.putAll(list, into: entries)
    }

    /// Deletes the entry numbered `id`.
    func erase(_ id: Int) async throws StorageWriteError {
        try await collections.delete(id, from: entries)
    }

    /// Deletes every entry.
    func eraseAll() async {
        do {
            try await collections.clear(entries)
        } catch _ {
            problem = "unavailable"
        }
        await refresh()
    }

    /// Asks the browser to keep the entries.
    func keep() async -> StoragePersistence {
        return await collections.requestPersistence()
    }
}
