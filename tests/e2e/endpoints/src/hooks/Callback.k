import k.auth.Session
import k.http.HttpResponse
import k.http.IncomingRequest

/// An endpoint that uses the session, like an OAuth callback: it receives
/// the cookie and is subject to the forgery check.
public page Callback at "/oauth/callback" {
    @Inject let session: Session

    @Get
    func land(request: IncomingRequest) async -> HttpResponse {
        let who = if session.user == nil { "guest" } else { "user" }

        return HttpResponse(status: .ok)
            .body(.text("callback as {who}"))
    }

    @Post
    func post(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
            .body(.text("posted"))
    }
}
