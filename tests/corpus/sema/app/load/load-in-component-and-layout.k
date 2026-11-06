// Components and layouts may use `@Load` too; each is its own loading boundary.
func count() async -> Int {
    return 1
}

component Sidebar {
    @Load let total: Int = await count()

    view {
        text { "{total}" }
    }

    loading {
    }
}

layout Shell {
    @Slot let content: View
    @Load let total: Int = await count()

    view {
        Sidebar()
        content
    }

    loading {
        text { "…" }
    }
}
