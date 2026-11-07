/// Renders the in-process outcomes on the server.
public page Report {
    @Inject let client: VaultClient

    @Load let text: String = await client.report()

    view {
        paragraph { "report: {text}" }
    }

    loading {
        paragraph { "loading" }
    }
}
