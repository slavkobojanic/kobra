// `untracked` works in a `@Derived` initializer too.
component Tracker {
    @State var page: Int = 0
    @State var session: String = ""
    @Derived let label: String = "{page} {untracked { session }}"

    view {
        text { label }
    }
}
