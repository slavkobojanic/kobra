/// An HTTP status (`docs/design/32-http-client.md`, "HttpStatus"). Named
/// statuses are cases; any other code is `.other(Int)`, which never holds a
/// code that has a name (`.other(503)` is a compile error).
public enum HttpStatus implements Equatable, Hashable, Codable {
    /// The request succeeded: OK.
    ///
    /// 200.
    case ok
    /// A resource was created: Created.
    ///
    /// 201.
    case created
    /// The request was accepted for later processing: Accepted.
    ///
    /// 202.
    case accepted
    /// The request succeeded with no body: No Content.
    ///
    /// 204.
    case noContent
    /// The resource has a new permanent URL: Moved Permanently.
    ///
    /// 301.
    case movedPermanently
    /// The answer is at another URL, fetched with `GET`: See Other.
    ///
    /// 303.
    case seeOther
    /// The cached copy is still valid: Not Modified.
    ///
    /// 304.
    case notModified
    /// The request is malformed: Bad Request.
    ///
    /// 400.
    case badRequest
    /// The request needs authentication: Unauthorized.
    ///
    /// 401.
    case unauthorized
    /// The caller may not do this: Forbidden.
    ///
    /// 403.
    case forbidden
    /// Nothing exists at the URL: Not Found.
    ///
    /// 404.
    case notFound
    /// The URL doesn't accept the method: Method Not Allowed.
    ///
    /// 405.
    case methodNotAllowed
    /// The request conflicts with the current state: Conflict.
    ///
    /// 409.
    case conflict
    /// The resource existed but was removed: Gone.
    ///
    /// 410.
    case gone
    /// The body is larger than the server accepts: Payload Too Large.
    ///
    /// 413.
    case payloadTooLarge
    /// The body's content type isn't accepted: Unsupported Media Type.
    ///
    /// 415.
    case unsupportedMediaType
    /// The body is well-formed but invalid: Unprocessable Entity.
    ///
    /// 422.
    case unprocessableEntity
    /// The caller exceeded a rate limit: Too Many Requests.
    ///
    /// 429.
    case tooManyRequests
    /// The server failed: Internal Server Error.
    ///
    /// 500.
    case internalServerError
    /// An upstream server answered badly: Bad Gateway.
    ///
    /// 502.
    case badGateway
    /// The server can't answer for now: Service Unavailable.
    ///
    /// 503.
    case serviceUnavailable
    /// An upstream server didn't answer in time: Gateway Timeout.
    ///
    /// 504.
    case gatewayTimeout
    /// Any code without a named case; the payload is the code, never one
    /// that has a name.
    case other(Int)

    /// A status from an integer literal, validated to `100...599` at compile
    /// time: `HttpStatus(code: 418)` is `.other(418)`.
    @Native
    @Literal
    init(code: Int) {}

    /// The numeric code.
    @Native
    var code: Int
    /// `true` for a 2xx (success) status.
    @Native
    var isSuccess: Bool
    /// `true` for a 4xx (client error) status.
    @Native
    var isClientError: Bool
    /// `true` for a 5xx (server error) status.
    @Native
    var isServerError: Bool
}
