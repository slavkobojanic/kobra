import k.http.HttpResponse
import k.http.IncomingRequest

/// Response shapes: redirects, other status codes, a forbidden header and a panic.
public page Responses at "/responses/{kind}" {
    @Param let kind: String

    @Get
    func respond(request: IncomingRequest) async -> HttpResponse {
        if kind == "home" {
            return HttpResponse.redirect(to: .home)
        }

        if kind == "away" {
            return HttpResponse.redirect(url: Url("https://example.com/away"))
        }

        if kind == "teapot" {
            return HttpResponse(status: .other(418))
                .body(.text("short and stout"))
        }

        if kind == "header" {
            let name = request.query["name"] ?? "x-custom"

            return HttpResponse(status: .ok)
                .headers([name: "a=b"])
                .body(.text("header set"))
        }

        if kind == "panic" {
            let empty: List<String> = []

            return HttpResponse(status: .ok)
                .body(.text(empty[0]))
        }

        return HttpResponse(status: .notFound)
            .body(.text("no such kind"))
    }
}
