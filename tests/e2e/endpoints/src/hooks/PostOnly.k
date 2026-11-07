import k.http.HttpResponse
import k.http.IncomingRequest

/// A POST-only endpoint.
public page PostOnly at "/post-only" {
    @Post
    func receive(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .accepted)
    }
}
