import k.http.HttpClient
import k.http.HttpRequest

/// Calls the third-party API and describes every outcome as text, so the
/// request script can check each `HttpError` case.
public service Upstream {
    @Inject let http: HttpClient
    @Inject let config: UpstreamConfig

    /// `fetchJson` of `path`, described.
    func describe(path: String, timeout: Duration) async -> String {
        let request = HttpRequest(method: .get, url: config.url.appending(path: path)).timeout(timeout)

        do {
            let forecast: Forecast = try await http.fetchJson(request: request)

            return "forecast {forecast.temperature} {forecast.unit}"
        } catch .offline {
            return "offline"
        } catch .timeout {
            return "timeout"
        } catch .status(let status, let body) {
            return "status {status.code} {body}"
        } catch .decoding(_) {
            return "decoding"
        }
    }

    /// `send` of `path`: the status and body, whatever the status.
    func raw(path: String) async -> String {
        let request = HttpRequest(method: .get, url: config.url.appending(path: path))

        do {
            let response = try await http.send(request: request)
            let named = response.status == .serviceUnavailable

            return "{response.status.code} success={response.isSuccess} named={named} {response.body}"
        } catch _ {
            return "failed"
        }
    }

    /// A request to a port nothing listens on.
    func unreachable() async -> String {
        let request = HttpRequest(method: .get, url: config.offlineUrl.appending(path: "/v1/forecast"))

        do {
            let response = try await http.send(request: request)

            return "answered {response.status.code}"
        } catch .offline {
            return "offline"
        } catch _ {
            return "other"
        }
    }

    /// A POST with a JSON body, a bearer token and a header; then what the
    /// fake saw.
    func create() async -> String {
        let request = HttpRequest(method: .post, url: config.url.appending(path: "/v1/things"))
            .bearerToken(config.token)
            .headers(["idempotency-key": "thing-1"])
            .query(["expand": "all"])
            .body(.form(["name": "K Mug", "quantity": "2"]))

        do {
            let created: Created = try await http.fetchJson(request: request)
            let seen: List<FakeRequest> = try await http.fetchJson(
                request: HttpRequest(method: .get, url: config.url.appending(path: "/_fake/requests"))
            )
            let last = seen[seen.count - 1]
            let auth = last.headers["authorization"] ?? "none"
            let key = last.headers["idempotency-key"] ?? "none"
            let type = last.headers["content-type"] ?? "none"

            return "{created.id} {last.method} {last.path} auth={auth} key={key} type={type} body={last.body}"
        } catch _ {
            return "failed"
        }
    }
}
