import k.http.HttpResponse
import k.http.IncomingRequest

/// One handler per method; the others are 405 with `Allow`.
public page Items at "/items" {
    @Get
    func list(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
            .body(.text("listed"))
    }

    @Post
    func create(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .created)
            .body(.text("created"))
    }

    @Delete
    func clear(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .noContent)
    }
}
