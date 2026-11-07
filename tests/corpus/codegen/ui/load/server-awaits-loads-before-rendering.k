// The server renderer awaits every load before rendering `view`, then serializes the values for
// hydration, so the browser never fetches them again (14-async-data.md, "Server rendering").
struct Greeting implements Codable {
    let text: String
}

service Greeter {
    func greet() async -> Greeting {
        return Greeting(text: "hi")
    }
}

page Hello {
    @Inject let greeter: Greeter

    @Load let greeting: Greeting = await greeter.greet()

    view {
        paragraph { greeting.text }
    }

    loading {
        paragraph { "…" }
    }
}
