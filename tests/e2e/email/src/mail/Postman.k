import k.http.HttpClient
import k.http.HttpRequest
import k.server.Email
import k.server.EmailProvider
import k.text.EmailAddress

/// Sends messages and reports each `EmailError` case as text.
public service Postman {
    @Inject let mailer: EmailProvider
    @Inject let http: HttpClient
    @Inject let fake: FakeConfig

    func send(to: EmailAddress, subject: String, html: Bool) async -> String {
        var email = Email(to: to, subject: subject, text: "Thanks for your order!")
            .replyTo(EmailAddress("support@shop.example"))
            .cc([EmailAddress("copy@shop.example")])

        if html {
            email = email.html("<p>Thanks for your order!</p>")
        }

        do {
            try await mailer.send(email: email)

            return "sent"
        } catch .unavailable {
            return "unavailable"
        } catch .rejected(let reason) {
            return "rejected {reason}"
        } catch .invalidMessage(let reason) {
            return "invalid {reason}"
        }
    }

    func inbox() async -> String {
        do {
            let mail: List<ReceivedMail> = try await http.fetchJson(
                request: HttpRequest(method: .get, url: fake.url.appending(path: "/_fake/mail"))
            )
            let lines = mail.map { m => "{m.from} -> {m.to.joined(separator: ",")}: {m.subject} html={m.data.contains("text/html")} replyTo={m.data.contains("Reply-To: support@shop.example")}" }

            return lines.joined(separator: " | ")
        } catch _ {
            return "no inbox"
        }
    }
}
