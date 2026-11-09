import k.crypto.Uuid
import k.rpc.RpcOnlyError

/// The server implementation of `MemosProtocol`. The server stores and
/// serves a recording's bytes; it can't decode or inspect media
/// (`docs/design/46-media-and-rtc.md`, "Server and client").
public service MemosApi implements MemosProtocol {
    @Inject let memos: MemoRepository

    /// The longest memo the server accepts.
    let longest: Duration = 3.minutes

    rpc list() async throws RpcOnlyError -> List<MemoSummary> {
        let all = await memos.newestFirst()
        return all.map(summarize)
    }

    rpc audio(id: Uuid) async throws MemoError -> Bytes {
        guard let memo = await memos.find(id: id) else {
            throw MemoError.notFound
        }

        guard let audio = Bytes.parse(base64: memo.audioBase64) else {
            throw MemoError.notFound
        }

        return audio
    }

    rpc upload(recordedAt: Date, duration: Duration, mediaType: String, audio: Bytes) async throws UploadError -> MemoSummary {
        // A second of slack: the browser's clock and the encoder disagree a little.
        guard audio.count > 0 and duration <= longest + 1.seconds and mediaType.startsWith("audio/") else {
            throw UploadError.notAMemo
        }

        let memo = Memo(
            id: Uuid.random(),
            recordedAt: recordedAt,
            durationMilliseconds: duration.inMilliseconds,
            mediaType: mediaType,
            size: audio.count,
            audioBase64: audio.base64
        )
        await memos.insert(memo)

        return summarize(memo: memo)
    }

    rpc delete(id: Uuid) async throws RpcOnlyError {
        if let memo = await memos.find(id: id) {
            await memos.delete(memo)
        }
    }

    /// The memo without its audio.
    private func summarize(memo: Memo) -> MemoSummary {
        return MemoSummary(
            id: memo.id,
            recordedAt: memo.recordedAt,
            duration: memo.durationMilliseconds.milliseconds,
            mediaType: memo.mediaType,
            size: memo.size
        )
    }
}
