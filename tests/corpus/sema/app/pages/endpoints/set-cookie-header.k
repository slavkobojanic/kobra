// Cookies are set only through `Session` (27-endpoint-pages.md): a literal
// `set-cookie` header name, in any case, is K5013 at the key. Other headers
// and a computed name (which panics at run time instead) are not.
import k.http.HttpResponse
import k.http.IncomingRequest

page Hook at "/hook" {
    @Post
    func run(request: IncomingRequest) async -> HttpResponse {
        let name = request.headers["x-name"] ?? "x-other"
        let other = HttpResponse(status: .ok).headers([name: "a=b", "cache-control": "no-store"])

        return HttpResponse(status: .ok)
            .headers(["set-cookie": "id=1", "x-trace": "abc"])
            .headers(["Set-Cookie": "id=2"])
    }
}
