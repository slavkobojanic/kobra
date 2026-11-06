import k.http.HttpStatus

/// The entries of a `metadata { }` block (`docs/design/22-metadata.md`).
///
/// A metadata block's body is checked with an implicit receiver of this
/// type, so `title("…")` resolves to `MetadataEntries.title`. Which entries
/// are allowed where (`status` only inside `failed`, `manifest`, `themeColor`
/// and `locale` only in `App.k`) is checked by the compiler.
@Native
public class MetadataEntries {
    /// The document title.
    func title(_ text: String)
    /// The description; over 160 characters is a warning.
    func description(_ text: String)
    /// The canonical URL, rendered absolute.
    func canonical(_ page: Page)
    /// Open Graph and similar previews.
    func socialPreview(image: Url, title: String)
    /// A schema.org entry.
    func structuredData(_ data: StructuredData)
    /// The page icon.
    func icon(_ url: Url)
    /// The web manifest; `App.k` only.
    func manifest(name: String, shortName: String)
    /// The browser theme color; `App.k` only.
    func themeColor(_ color: Color)
    /// The locale for formatting.
    func locale(_ locale: Locale)
    /// The HTTP status of a server-rendered `failed` page; only in a
    /// `metadata` block inside `failed`.
    func status(_ status: HttpStatus)
}
