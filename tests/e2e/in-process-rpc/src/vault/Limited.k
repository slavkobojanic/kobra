/// Calls a rate-limited rpc in-process on every render.
public page Limited {
    @Inject let client: VaultClient

    @Load let text: String = await client.limited()

    view {
        paragraph { "limited: {text}" }
    }

    loading {
        paragraph { "loading" }
    }
}
