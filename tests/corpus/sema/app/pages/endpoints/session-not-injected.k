// An endpoint page whose dependencies reach `SessionManager` must inject `Session` itself, so its
// use of cookies is visible on the page.
import k.auth.Session
import k.auth.SessionManager
import k.http.HttpResponse
import k.http.IncomingRequest

service SignInFlow {
    @Inject let sessions: SessionManager
}

page OAuthCallback at "/oauth/callback" {
    @Inject let flow: SignInFlow

    @Get
    func receive(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}

page OAuthReturn at "/oauth/return" {
    @Inject let session: Session
    @Inject let flow: SignInFlow

    @Get
    func receive(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}
