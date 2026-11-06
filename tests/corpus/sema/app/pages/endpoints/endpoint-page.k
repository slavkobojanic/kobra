// An endpoint page: a page with HTTP handlers instead of a view, a named URL, server-only.
import k.http.HttpResponse
import k.http.IncomingRequest

page StripeWebhook at "/webhooks/stripe" {
//   ^^^^^^^^^^^^^ side: server-only
    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .noContent)
    }

    @Get
    func check(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}
