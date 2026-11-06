/// An HTTP request method.
public enum HttpMethod implements Equatable, Hashable, Codable {
    /// `GET`.
    case get
    /// `POST`.
    case post
    /// `PUT`.
    case put
    /// `PATCH`.
    case patch
    /// `DELETE`.
    case delete
    /// `HEAD`.
    case head
}
