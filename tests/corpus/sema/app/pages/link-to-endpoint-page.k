// Endpoint pages are `Endpoint` cases, not `Page` cases, so `link(to:)` can't name one.
import k.http.HttpResponse
import k.http.IncomingRequest

page Health at "/health" {
    @Get
    func check(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}

page Status {
    view {
        link(to: .health) { "Health" }
    }
}
