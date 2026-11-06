// A rate-limited page.
import k.auth.RateLimit

public page Search requires RateLimit(10.perMinute) {
    view {
        text { "search" }
    }
}
