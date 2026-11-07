import k.http.HttpResponse
import k.http.IncomingRequest

/// An endpoint whose handler calls the page's own functions: a plain one
/// and an async one, both reading the request's `@Param`-free state.
public page Helper at "/helper" {
    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        let size = await measure(request: request)

        return HttpResponse(status: .ok)
            .body(.text(describe(size: size)))
    }

    /// The body's size in bytes.
    func measure(request: IncomingRequest) async -> Int {
        return request.rawBody.count
    }

    /// A short report of `size`.
    func describe(size: Int) -> String {
        return "{size} bytes"
    }
}
