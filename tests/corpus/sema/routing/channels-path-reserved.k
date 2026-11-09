// `/_k/channels` is where the browser opens its channels' WebSocket, so no page or endpoint may have it.
import k.http.HttpResponse
import k.http.IncomingRequest

page Channels at "/_k/channels" {
    @Get
    func check(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}

page Status at "/_k/channels/status" {
    view {
        paragraph { "fine" }
    }
}
