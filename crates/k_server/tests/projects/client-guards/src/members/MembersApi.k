import k.auth.Session
import k.auth.SessionManager
import k.rpc.RpcOnlyError

public service MembersApi implements MembersProtocol {
    @Inject let sessions: SessionManager
    @Inject let session: Session

    rpc signIn(name: String, admin: Bool) async throws RpcOnlyError {
        let roles: Set<MemberRole> = if admin { Set([MemberRole.admin]) } else { Set([]) }
        await sessions.signIn(userId: MemberId(name: name), roles: roles)
    }

    rpc signOut() async throws RpcOnlyError {
        await sessions.signOut()
    }

    rpc greeting() async throws RpcOnlyError -> String {
        return "hello {session.user.id.name}"
    }
}
