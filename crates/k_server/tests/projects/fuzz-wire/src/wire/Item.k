import k.http.HttpResponse
import k.http.IncomingRequest

/// A typed path parameter: a segment that isn't an `Int` is a 404.
public page Item at "/items/{id}" {
    @Param let id: Int

    @Get
    func show(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
            .body(.text("item {id}"))
    }
}
