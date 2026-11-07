// Endpoint pages are server-only: neither their URL nor their handler reaches the client, while
// view pages are in both route tables (12-routing.md, "Endpoint pages"; codegen.md §14).
import k.http.HttpResponse
import k.http.IncomingRequest

page Health at "/health/check" {
    @Get
    func check(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}

page Home {
    view {
        text { "home" }
    }
}
