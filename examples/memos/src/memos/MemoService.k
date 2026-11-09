import k.device.File
import k.media.Recording
import k.rpc.RpcOnlyError

/// The memos in the browser: the list, and storing and deleting memos.
///
/// It injects only `MemosProtocol`, so it runs on both sides: the list it
/// holds is empty while the server renders, and the page loads it once it
/// runs in the browser.
public service MemoService {
    @Inject let api: MemosProtocol

    /// Every memo, newest first.
    @State var items: List<MemoSummary> = []

    /// Whether the list has been loaded.
    @State var isLoaded: Bool = false

    /// Loads the list.
    ///
    /// - Throws: `RpcOnlyError.rpcFailure` when the server is out of reach.
    func load() async throws RpcOnlyError {
        items = try await api.list()
        isLoaded = true
    }

    /// Stores a finished recording, which then heads the list.
    ///
    /// - Parameter recordedAt: when the recording started.
    /// - Throws: `UploadError.notAMemo` for a recording the server refuses.
    func upload(_ recording: Recording, recordedAt: Date) async throws UploadError {
        let memo = try await api.upload(recordedAt: recordedAt, duration: recording.duration, mediaType: recording.file.mediaType, audio: recording.file.bytes)
        items = [memo] + items
    }

    /// A memo's recording, as a file to play or save.
    ///
    /// - Throws: `MemoError.notFound` when it was deleted.
    func file(of memo: MemoSummary) async throws MemoError -> File {
        let bytes = try await api.audio(id: memo.id)
        return File(name: memo.fileName(), mediaType: memo.mediaType, bytes: bytes)
    }

    /// Deletes a memo.
    ///
    /// - Throws: `RpcOnlyError.rpcFailure` when the server is out of reach.
    func delete(_ memo: MemoSummary) async throws RpcOnlyError {
        try await api.delete(id: memo.id)
        items = items.filter { other => other.id != memo.id }
    }
}
