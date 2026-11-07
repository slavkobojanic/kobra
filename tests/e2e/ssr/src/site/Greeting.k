/// Escaping of parameters in text and attributes.
public page Greeting {
    @Param let name: String = "world"

    metadata {
        title("Hello {name}")
    }

    view {
        heading(level: 1) { "Hello, {name}" }

        link(to: .greeting.name(name)) { "Again, {name}" }

        image(source: Url("https://example.com/wave.png"), description: "A wave for {name}")
    }
}
