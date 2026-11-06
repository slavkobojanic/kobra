import admin.AdminConfig
import admin.StaffRole
import k.auth.Session
import k.http.HttpRequest
import k.http.HttpResponse
import k.test.TestClient

// The page guards, checked over HTTP: they run on the server before the page
// loads anything, so a server response is the thing to test. The test acts as
// one browser, so `session.signIn` here applies to the client's requests.

test Admin {
    use AdminConfig.sample for AdminConfig

    @Inject let client: TestClient
    @Inject let session: Session

    func openDashboard() async -> HttpResponse {
        return await client.send(request: HttpRequest(method: .get, url: Page.admin.absoluteUrl))
    }

    test "a visitor without a session is sent to sign in, and back afterwards" {
        let response = await openDashboard()

        expect response.status == 303
        expect response.headers["location"] == "{Page.signIn.returnTo(.admin).url}"
    }

    test "a signed-in admin sees the dashboard" {
        await session.signIn(userId: "admin@shop.test", roles: Set([StaffRole.admin]))

        let response = await openDashboard()

        expect response.status == 200
        expect response.body.contains("<h1>Orders</h1>")
    }

    test "a session without the admin role is forbidden" {
        await session.signIn(userId: "someone@shop.test", roles: Set([]))

        let response = await openDashboard()

        expect response.status == 403
    }
}
