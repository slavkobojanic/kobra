import k.http.HttpResponse
import k.http.IncomingRequest

/// A `String` path parameter, percent-decoded.
public page FileInfo at "/files/{name}" {
    @Param let name: String

    @Get
    func info(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
            .body(.text("file [{name}]"))
    }
}
