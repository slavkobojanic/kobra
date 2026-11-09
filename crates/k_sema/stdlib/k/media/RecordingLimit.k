/// Where a recording stops by itself. The default size is
/// `File.maximumSize`, 16 MiB, so every recording fits in a `File`.
public struct RecordingLimit implements Equatable {
    /// The largest file, in bytes.
    let size: Int = 16777216
    /// The longest recording; `nil` for none.
    let duration: Duration? = nil
}
