// `structuredData(…)` in a page's `metadata` reaches the server's document
// head as JSON-LD (22-metadata.md): the call is kept on the render's
// metadata, with the `StructuredData` value.
page Mug {
    metadata {
        title("Mug")
        structuredData(.product(name: "Mug", price: 24.00.usd))
    }

    view {
        text { "mug" }
    }
}
