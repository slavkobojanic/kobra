import k.crypto.Uuid
import k.rpc.RpcOnlyError

/// The contract between the browser and the server for the memos.
///
/// A memo travels whole in one request, so a recording must fit in the
/// 1 MB request limit: `MemoRecorder` stops at three minutes or 750 kB,
/// whichever comes first.
public protocol MemosProtocol {
    /// Every memo, newest first, without its audio.
    rpc list() throws RpcOnlyError -> List<MemoSummary>

    /// One memo's recording.
    ///
    /// - Throws: `MemoError.notFound` when it was deleted.
    rpc audio(id: Uuid) throws MemoError -> Bytes

    /// Stores a recording.
    ///
    /// - Parameters:
    ///   - recordedAt: when the recording started.
    ///   - duration: how long it is.
    ///   - mediaType: what the browser recorded.
    ///   - audio: the recording.
    /// - Throws: `UploadError.notAMemo` for an empty, overlong or non-audio
    ///   recording.
    rpc upload(recordedAt: Date, duration: Duration, mediaType: String, audio: Bytes) throws UploadError -> MemoSummary

    /// Deletes a memo. Deleting one that is already gone does nothing.
    rpc delete(id: Uuid) throws RpcOnlyError
}
