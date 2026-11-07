import k.http.HttpStatus

/// The entries of a `metadata { }` block (`docs/design/22-metadata.md`).
///
/// A metadata block's body is checked with an implicit receiver of this
/// type, so `title("…")` resolves to `MetadataEntries.title`. Which entries
/// are allowed where (`status` only inside `failed`, `manifest`, `themeColor`
/// and `locale` only in `App.k`) is checked by the compiler.
@Native
public class MetadataEntries {
    /// Sets the document title.
    func title(_ text: String)
    /// Sets the description; over 160 characters is a warning.
    func description(_ text: String)
    /// Sets the canonical URL to `page`'s, rendered absolute.
    func canonical(_ page: Page)
    /// Sets the Open Graph (and similar) preview shown when the page is
    /// shared.
    ///
    /// - Parameters:
    ///   - image: the preview image.
    ///   - title: the preview title.
    func socialPreview(image: Url, title: String)
    /// Adds a schema.org entry ([`StructuredData`]).
    func structuredData(_ data: StructuredData)
    /// Sets the page icon.
    func icon(_ url: Url)
    /// Sets the web manifest; `App.k` only.
    ///
    /// - Parameters:
    ///   - name: the app's full name.
    ///   - shortName: the name under the home-screen icon.
    func manifest(name: String, shortName: String)
    /// Sets the browser theme color; `App.k` only.
    func themeColor(_ color: Color)
    /// Sets the locale for formatting ([`Locale`]); `App.k` only.
    func locale(_ locale: Locale)
    /// Sets the HTTP status of a server-rendered `failed` page
    /// ([`k.http.HttpStatus`]); only in a `metadata` block inside `failed`.
    func status(_ status: HttpStatus)
}
