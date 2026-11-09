/// Who is at the other end, sent once on the `introductions` data channel
/// when the call connects, so the page can say "Sam's camera".
public struct Introduction implements Codable, Equatable {
    /// The person's name.
    let name: String
}
