// `@Param` types that can be read from a URL: `String`, `Int`, `Uuid`, `Page`, enums without
// payloads, and optionals of these.
import k.crypto.Uuid

enum Sort {
    case newest
    case oldest
}

page Search {
    @Param let query: String
    @Param let page: Int
    @Param let id: Uuid
    @Param let returnTo: Page
    @Param let sort: Sort = .newest
    //         ^^^^ type: corpus.Sort
    @Param let tag: String? = nil

    view {
        text { query }
    }
}
