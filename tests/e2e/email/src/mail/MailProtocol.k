import k.rpc.RpcOnlyError
import k.text.EmailAddress

public protocol MailProtocol {
    rpc send(to: EmailAddress, subject: String, html: Bool) throws RpcOnlyError -> String
    rpc inbox() throws RpcOnlyError -> String
}
