/// An HTTP request method.
public enum HttpMethod implements Equatable, Hashable, Codable {
    /// The `GET` method: read a resource.
    case get
    /// The `POST` method: submit data or create a resource.
    case post
    /// The `PUT` method: replace a resource.
    case put
    /// The `PATCH` method: change part of a resource.
    case patch
    /// The `DELETE` method: remove a resource.
    case delete
    /// The `HEAD` method: `GET` without the body.
    case head
}
