// Declaring a member named `reload` next to `@Load` fields is an error; without them it's an ordinary
// name.
func count() async -> Int {
    return 1
}

page Stats {
    @Load let total: Int = await count()

    func reload() {
    }

    view {
        text { "{total}" }
    }

    loading {
    }
}

page Plain {
    func reload() {
    }

    view {
        button { "Reload" }
            .on(click: reload)
    }
}
