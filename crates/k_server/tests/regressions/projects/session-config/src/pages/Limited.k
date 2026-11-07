import k.auth.RateLimit

/// A rate-limited public page.
public page Limited requires RateLimit(2.perMinute) {
    view {
        paragraph { "limited page" }
    }
}
