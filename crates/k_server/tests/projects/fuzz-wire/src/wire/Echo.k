import k.http.HttpResponse
import k.http.IncomingRequest

/// A session-free endpoint page that echoes the raw body and one header.
public page Echo at "/hooks/echo" {
    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        let tag = request.headers["x-tag"] ?? "none"

        return HttpResponse(status: .ok)
            .headers(["x-tag-seen": tag])
            .body(.bytes(request.rawBody, contentType: "application/octet-stream"))
    }
}
