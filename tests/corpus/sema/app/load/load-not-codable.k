// `@Load` values are serialized to the browser, so they are `Codable`.
struct Chart {
    let points: List<Int>
}

func chart() async -> Chart {
    return Chart(points: [])
}

func total() async -> Int {
    return 0
}

page Stats {
    @Load let chart: Chart = await chart()
    @Load let total: Int = await total()

    view {
        text { "{total}" }
    }

    loading {
    }
}
