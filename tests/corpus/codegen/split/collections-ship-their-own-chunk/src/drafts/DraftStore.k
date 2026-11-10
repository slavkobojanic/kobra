import k.storage.Collection
import k.storage.CollectionIndex
import k.storage.Collections

public service DraftStore {
    @Inject let collections: Collections

    private let drafts: Collection<Int, Draft> = Collection(name: "drafts", key: "id")
    private let bySavedAt: CollectionIndex<Date, Draft> = CollectionIndex(collection: "drafts", field: "savedAt")

    @State var count: Int = 0

    func load() async {
        do {
            count = try await collections.count(in: drafts)
            let newest = try await collections.range(in: bySavedAt, reversed: true, limit: 1)
            count = count + newest.count
        } catch _ {
        }
    }
}
