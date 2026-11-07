import k.http.HttpResponse
import k.http.IncomingRequest

/// A webhook that doesn't use the session: it echoes the raw body and
/// reports the headers it was given.
public page Echo at "/webhooks/echo" {
    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        let signature = request.headers["x-signature"] ?? "none"
        let cookie = request.headers["cookie"] ?? "none"

        return HttpResponse(status: .ok)
            .headers(["x-signature-seen": signature, "x-cookie-seen": cookie])
            .body(.bytes(request.rawBody, contentType: "application/octet-stream"))
    }
}
