// Handlers have the fixed signature `func name(request: IncomingRequest) async -> HttpResponse`.
import k.http.HttpResponse
import k.http.IncomingRequest

enum FeedError implements Error {
    case broken
}

page Feed at "/feed" {
    @Get
    func list(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }

    @Post
    func add(request: IncomingRequest) async throws FeedError -> HttpResponse {
        throw FeedError.broken
    }

    @Put
    func replace(body: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }

    @Delete
    func remove(request: IncomingRequest) -> HttpResponse {
        return HttpResponse(status: .ok)
    }

    @Patch
    func edit(request: IncomingRequest) async -> String {
        return "ok"
    }
}
