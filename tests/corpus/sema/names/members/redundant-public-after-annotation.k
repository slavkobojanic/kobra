// The modifier follows the annotations; `public` after `@State` is K1302 like on any other member.
component Counter {
    @State public var count = 0

    view {
        paragraph { "{count}" }
    }
}
