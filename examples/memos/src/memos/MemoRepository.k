import k.crypto.Uuid

/// Queries for `Memo`.
public repository MemoRepository for Memo {
    /// Every memo, newest first.
    func newestFirst() async -> List<Memo> {
        return await memos.sortedByDescending { memo => memo.recordedAt }
    }

    /// One memo, or `nil` when there is no such memo.
    func find(id: Uuid) async -> Memo? {
        return await memos.first { memo => memo.id == id }
    }
}
