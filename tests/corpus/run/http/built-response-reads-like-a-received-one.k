// A response built with `HttpResponse(status:)` (as a test fake of
// `HttpClient` does) reads exactly like one an `HttpClient` received:
// its status, success, text, headers and JSON.
import k.http.HttpResponse

struct Point implements Codable {
    let x: Int
    let y: Int
}

/// The decoded point, or the `HttpError` case that refused it.
func decode(_ r: HttpResponse) -> String {
    do {
        let p: Point = try r.json()
        return "{p.x},{p.y}"
    } catch .decoding(_) {
        return "decoding"
    } catch .offline {
        return "offline"
    } catch .timeout {
        return "timeout"
    } catch .status(_, _) {
        return "status"
    }
}

func main() -> List<String> {
    let missing = HttpResponse(status: .notFound).body(.text("gone"))
    let found = HttpResponse(status: .ok)
        .headers(["X-Trace": "abc"])
        .body(.json(Point(x: 1, y: 2)))
    return [
        "{missing.isSuccess} {missing.body}",
        "{found.isSuccess} {found.body}",
        found.headers["x-trace"] ?? "none",
        found.headers["content-type"] ?? "none",
        decode(found),
        decode(missing),
    ]
}
