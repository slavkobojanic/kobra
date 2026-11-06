/// An HTTP status (`docs/design/32-http-client.md`, "HttpStatus"). Named
/// statuses are cases; any other code is `.other(Int)`, which never holds a
/// code that has a name (`.other(503)` is a compile error).
public enum HttpStatus implements Equatable, Hashable, Codable {
    /// 200.
    case ok
    /// 201.
    case created
    /// 202.
    case accepted
    /// 204.
    case noContent
    /// 301.
    case movedPermanently
    /// 303.
    case seeOther
    /// 304.
    case notModified
    /// 400.
    case badRequest
    /// 401.
    case unauthorized
    /// 403.
    case forbidden
    /// 404.
    case notFound
    /// 405.
    case methodNotAllowed
    /// 409.
    case conflict
    /// 410.
    case gone
    /// 413.
    case payloadTooLarge
    /// 415.
    case unsupportedMediaType
    /// 422.
    case unprocessableEntity
    /// 429.
    case tooManyRequests
    /// 500.
    case internalServerError
    /// 502.
    case badGateway
    /// 503.
    case serviceUnavailable
    /// 504.
    case gatewayTimeout
    /// Any code without a named case.
    case other(Int)

    /// A status from an integer literal, validated to `100...599` at compile
    /// time: `HttpStatus(code: 418)` is `.other(418)`.
    @Native
    @Literal
    init(code: Int) {}

    /// The numeric code.
    @Native
    var code: Int
    /// `true` for 2xx.
    @Native
    var isSuccess: Bool
    /// `true` for 4xx.
    @Native
    var isClientError: Bool
    /// `true` for 5xx.
    @Native
    var isServerError: Bool
}
