// An endpoint page may hold helper functions next to its handlers.
import k.http.HttpResponse
import k.http.IncomingRequest

page Feed at "/feed" {
    @Get
    func list(request: IncomingRequest) async -> HttpResponse {
        return respond(text: "ok")
    }

    func respond(text: String) -> HttpResponse {
        return HttpResponse(status: .ok)
            .body(.text(text))
    }
}
