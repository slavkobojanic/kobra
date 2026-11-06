// Which declarations are server-only (sema.md §12.6).
import k.auth.SessionManager
import k.db.Database
import k.rpc.RpcOnlyError
import k.server.EmailProvider

entity Note {
//     ^^^^ side: server-only
    @Id let id: Int
}

repository NoteRepository for Note {}
//         ^^^^^^^^^^^^^^ side: server-only

config MailConfig {
//     ^^^^^^^^^^ side: server-only
    @Env("MAIL_FROM") let from: String
}

protocol NotesProtocol {
    rpc count() throws RpcOnlyError -> Int
}

service NotesApi implements NotesProtocol {
//      ^^^^^^^^ side: server-only
    rpc count() async throws RpcOnlyError -> Int {
        return 0
    }
}

@Scope(.request)
service RequestLog {}
//      ^^^^^^^^^^ side: server-only

@Scope(.server)
service Counters {}
//      ^^^^^^^^ side: server-only

service NoteService {
//      ^^^^^^^^^^^ side: server-only
    @Inject let notes: NoteRepository
}

service Mail {
//      ^^^^ side: server-only
    @Inject let config: MailConfig
    @Inject let provider: EmailProvider
}

service Health {
//      ^^^^^^ side: server-only
    @Inject let database: Database
}

service SignIn {
//      ^^^^^^ side: server-only
    @Inject let sessions: SessionManager
}

service Digest {
//      ^^^^^^ side: server-only
    @Inject let notes: NoteService
}
