// App and layout defaults don't apply to endpoint pages, so `@Layout` and `@NoLayout` are errors.
import k.http.HttpResponse
import k.http.IncomingRequest

layout Shop {
    @Slot let content: View

    view {
        content
    }
}

@Layout(Shop)
page Feed at "/feed" {
    @Get
    func run(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}

@NoLayout
page Ping at "/ping" {
    @Get
    func run(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}
