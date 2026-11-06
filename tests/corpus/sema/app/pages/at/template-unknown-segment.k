// A template segment that names no `@Param` is reported at the segment, and not as an unresolved name.
page Product at "/p/{slug}" {
    view {
        text { "product" }
    }
}
