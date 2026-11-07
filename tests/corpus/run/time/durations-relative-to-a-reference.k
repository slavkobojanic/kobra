// Plain functions take the current instant as a parameter, as code that injects no Clock must.
struct Signature {
    let signedAt: Date

    func isFresh(now: Date, tolerance: Duration) -> Bool {
        return now - signedAt <= tolerance
    }
}

func main() -> List<Bool> {
    let now = Date(unixSeconds: 1767268800)
    let recent = Signature(signedAt: now - 4.minutes)
    let stale = Signature(signedAt: now - 6.minutes)
    let exact = Signature(signedAt: now - 5.minutes)
    return [recent.isFresh(now: now, tolerance: 5.minutes), stale.isFresh(now: now, tolerance: 5.minutes), exact.isFresh(now: now, tolerance: 5.minutes)]
}
