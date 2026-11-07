// `if let name` in a `@Derived` initializer reads the state it names, so the derived value tracks it:
// the shorthand binds the value of `user`, not the signal holding it (codegen.md §6).
component Greeting {
    @State var user: String? = nil

    @Derived let label: String = if let user {
        "Hello, {user}"
    } else {
        "Hello"
    }

    view {
        text { label }
    }
}
