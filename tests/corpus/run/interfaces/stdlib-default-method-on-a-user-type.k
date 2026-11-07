// A user type implementing a stdlib interface gets the interface's default
// methods: a canned `HttpClient` (the shape of a test fake) answers
// `fetchJson`, which the stdlib defines over `send`.
import k.http.HttpClient
import k.http.HttpError
import k.http.HttpRequest
import k.http.HttpResponse

struct Point implements Codable {
    let x: Int
    let y: Int
}

class Canned implements HttpClient {
    func send(request: HttpRequest) async throws HttpError -> HttpResponse {
        if request.url.path == "/missing" {
            return HttpResponse(status: .notFound).body(.text("no such point"))
        }
        return HttpResponse(status: .ok).body(.json(Point(x: 3, y: 4)))
    }
}

func fetch(_ client: HttpClient, path: String) async -> String {
    do {
        let p: Point = try await client.fetchJson(request: HttpRequest(method: .get, url: Url("https://example.com{path}")))
        return "{p.x},{p.y}"
    } catch .status(_, let body) {
        return "status: {body}"
    } catch .decoding(let message) {
        return "decoding: {message}"
    } catch .offline {
        return "offline"
    } catch .timeout {
        return "timeout"
    }
}

func main() async -> List<String> {
    let client = Canned()
    return [await fetch(client, path: "/point"), await fetch(client, path: "/missing")]
}
