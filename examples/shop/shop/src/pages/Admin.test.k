import admin.AdminConfig
import admin.StaffId
import admin.StaffRole
import k.auth.SessionManager
import k.http.HttpRequest
import k.http.HttpResponse
import k.server.PublicUrls
import k.test.TestClient
import k.text.EmailAddress

// The page guards, checked over HTTP: they run on the server before the page
// loads anything, so a server response is the thing to test. The test acts as
// one browser, so `sessions.signIn` here applies to the client's requests.

test Admin {
    use AdminConfig.sample for AdminConfig

    @Inject let client: TestClient
    @Inject let sessions: SessionManager
    @Inject let urls: PublicUrls

    func openDashboard() async -> HttpResponse {
        return await client.send(request: HttpRequest(method: .get, url: urls.absolute(.admin)))
    }

    test "a visitor without a session is sent to sign in, and back afterwards" {
        let response = await openDashboard()

        expect response.status == .seeOther
        expect response.headers["location"] == "{Page.signIn.returnTo(.admin).url}"
    }

    test "a signed-in admin sees the dashboard" {
        await sessions.signIn(userId: StaffId.sample, roles: Set([StaffRole.admin]))

        let response = await openDashboard()

        expect response.status == .ok
        // The page's heading is styled, so it carries a class.
        expect response.body.contains("<title>Orders · K Shop admin</title>")
        expect response.body.contains(">Orders</h1>")
    }

    test "a session without the admin role is forbidden" {
        await sessions.signIn(userId: StaffId(email: EmailAddress("someone@shop.test")), roles: Set([]))

        let response = await openDashboard()

        expect response.status == .forbidden
    }
}
