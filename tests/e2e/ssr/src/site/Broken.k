/// A page whose load panics: a 500 with no details.
public page Broken {
    @Inject let articles: Articles

    @Load let value: String = try await articles.broken()

    view {
        paragraph { value }
    }

    loading {
    }
}
