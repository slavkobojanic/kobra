// A stdlib error enum: `HttpError` with a nested `HttpStatus`. `_` inside a
// pattern is fine, and `.status(let status, _)` binds the status's type.
import k.http.HttpError

func fetch() async throws HttpError -> String {
    return ""
}

func show() async -> String {
    do {
        return try await fetch()
    } catch .offline, .timeout {
        return "unreachable"
    } catch .status(.notFound, _) {
        return "missing"
    } catch .status(let status, _) {
//                      ^^^^^^ type: k.http.HttpStatus
        return "status {status}"
    } catch .decoding(let message) {
//                        ^^^^^^^ type: String
        return message
    }
}
