import k.http.HttpRequest
import k.server.PublicUrls
import k.test.TestClient

test HealthCheck {
    @Inject let client: TestClient
    @Inject let urls: PublicUrls

    test "paybox is healthy once its database answers" {
        let response = await client.send(request: HttpRequest(method: .get, url: urls.absolute(endpoint: .healthCheck)))

        expect response.status == .ok
    }
}
