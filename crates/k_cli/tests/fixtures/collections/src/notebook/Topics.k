/// Finding entries by topic. The first build has no index on `topic`;
/// `chrome.rs`' upgrade test replaces this file with `Topics.k.v2`, whose
/// index the browser adds to the database the first build left, filling
/// it from the entries already stored.
public service Topics {
    /// The entries about `topic`.
    func about(_ topic: String) async -> List<Entry> {
        return []
    }

    /// Whether this build indexes topics.
    let isIndexed: Bool = false
}
