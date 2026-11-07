/// A view page that loads a config-derived value through the protocol.
public page Welcome {
    @Inject let greetings: Greetings

    @Load let text: String = try await greetings.text()

    view {
        paragraph { text }
    }

    loading {
    }
}
