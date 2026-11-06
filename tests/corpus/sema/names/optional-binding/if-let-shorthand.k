// `if let name` means `if let name = name`: inside, `name` is the unwrapped local; after, the optional parameter again.
func greeting(name: String?) -> String {
    if let name {
        return "Hello, {name}"
        //              ^^^^ resolves: local name 3:12
    }
    return name ?? "Hello"
    //     ^^^^ resolves: param name 2:15
}
