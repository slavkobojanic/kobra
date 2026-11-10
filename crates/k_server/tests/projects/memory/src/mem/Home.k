/// The page the request leak test renders: a heading, a counter and a
/// list rendered on the server.
public page Home at "/" {
    @State var names: List<String> = ["Ada", "Grace", "Barbara", "Frances", "Hedy"]

    view {
        heading(level: 1) { "Memory" }

        Counter(label: "Visits")

        list {
            for name in names key name {
                item { "Hello, {name}" }
            }
        }
    }
}
