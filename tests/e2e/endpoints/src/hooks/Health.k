import k.http.HttpResponse
import k.http.IncomingRequest

/// A GET-only endpoint: HEAD follows GET, other methods are 405.
public page Health at "/health" {
    @Get
    func check(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
            .body(.text("ok"))
    }
}
