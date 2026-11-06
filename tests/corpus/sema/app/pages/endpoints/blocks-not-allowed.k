// Endpoint pages have no `metadata`, `loading`, `failed` or `style`.
import k.http.HttpResponse
import k.http.IncomingRequest

page Feed at "/feed" {
    @Get
    func run(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }

    metadata {
        title("Feed")
    }

    loading {
    }

    failed {
    }

    style {
    }
}
