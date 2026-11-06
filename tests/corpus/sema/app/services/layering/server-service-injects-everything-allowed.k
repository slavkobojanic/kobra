// A server-side service may inject services, protocols, platform interfaces, repositories, configs,
// `Database`, `SessionManager` and `Clock`.
import k.auth.SessionManager
import k.db.Database
import k.http.HttpClient
import k.rpc.RpcOnlyError
import k.time.Clock

protocol CatalogProtocol {
    rpc count() throws RpcOnlyError -> Int
}

entity Account {
    @Id let id: Int
}

repository AccountRepository for Account {}

config MailConfig {
    @Env("MAIL_FROM") let from: String
}

service Audit {
    @State var lines: List<String> = []
}

service Accounts {
//      ^^^^^^^^ side: server-only
    @Inject let audit: Audit
    @Inject let catalog: CatalogProtocol
    @Inject let http: HttpClient
    @Inject let accounts: AccountRepository
    @Inject let config: MailConfig
    @Inject let database: Database
    @Inject let sessions: SessionManager
    @Inject let clock: Clock
}
