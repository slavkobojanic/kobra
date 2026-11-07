import k.rpc.RpcOnlyError
import k.text.EmailAddress

public service MailApi implements MailProtocol {
    @Inject let postman: Postman

    rpc send(to: EmailAddress, subject: String, html: Bool) async throws RpcOnlyError -> String {
        return await postman.send(to: to, subject: subject, html: html)
    }

    rpc inbox() async throws RpcOnlyError -> String {
        return await postman.inbox()
    }
}
