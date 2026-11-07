// Interpolation formats each part by its type: `Double` through `$k.formatDouble` (so `2.0` keeps
// its decimal), `Int` and `Bool` as `String` would, an optional by its inner type or `nil`
// (codegen.md §6.4).
component Stats {
    @Prop let ratio: Double
    @Prop let count: Int
    @Prop let done: Bool
    @Prop let nickname: String?

    view {
        paragraph { "{ratio} {count} {done} {nickname}" }
    }
}
