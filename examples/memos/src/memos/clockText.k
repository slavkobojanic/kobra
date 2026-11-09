/// A duration as `m:ss`: `0:03`, `2:47`.
public func clockText(_ duration: Duration) -> String {
    let seconds = duration.inSeconds
    let rest = seconds % 60
    if rest < 10 {
        return "{seconds / 60}:0{rest}"
    }
    return "{seconds / 60}:{rest}"
}
