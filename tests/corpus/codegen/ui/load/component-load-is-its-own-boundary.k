// A component with `@Load` fields is its own loading boundary with its own group
// (14-async-data.md).
service Weather {
    func temperature() async -> Int {
        return 21
    }
}

component Sidebar {
    @Inject let weather: Weather

    @Load let degrees: Int = await weather.temperature()

    view {
        text { "{degrees}°" }
    }

    loading {
        text { "…" }
    }
}

page Home {
    view {
        main {
            Sidebar()
        }
    }
}
