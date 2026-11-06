// `untracked { … }` reads without subscribing, but the value is still
// needed to compute the field: a cycle through it is K3026 as well.
component Tracker {
    @Derived let first: Int = untracked { second } + 1
    @Derived let second: Int = first * 2

    view {
        text { "{first}" }
    }
}
