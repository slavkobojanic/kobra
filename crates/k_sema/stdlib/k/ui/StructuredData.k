/// A schema.org entry for `structuredData(...)` in `metadata`
/// ([`MetadataEntries.structuredData`]).
public enum StructuredData {
    /// A product with a name and a price.
    case product(name: String, price: Money)
    /// An article with a headline.
    case article(headline: String)
}
