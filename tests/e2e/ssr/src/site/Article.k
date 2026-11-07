/// A loaded article. Its text deliberately carries markup and a script end
/// tag, which the server must escape in HTML and in the `k-data` element.
public struct Article implements Codable {
    let slug: String
    let title: String
    let body: String
}
