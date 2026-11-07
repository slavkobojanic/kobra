/// The key of the (imaginary) search index.
public config SearchConfig {
    @Env("NOTES_SEARCH_SECRET") let key: Secret<String>
}
