import k.device.Files

/// Saves a memo as a download. `Files` is a client-only platform
/// interface, so this service is client-only; rows call it from a click.
public service MemoDownloads {
    @Inject let memos: MemoService
    @Inject let files: Files

    /// Fetches the memo's recording and saves it as a file named after it.
    ///
    /// - Returns: `false` when it couldn't be fetched or saved.
    func save(_ memo: MemoSummary) async -> Bool {
        do {
            let file = try await memos.file(of: memo)
            try files.save(file)
            return true
        } catch .notFound, .rpcFailure, .unavailable {
            return false
        }
    }
}
