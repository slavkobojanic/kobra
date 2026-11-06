// Repositories, `Database`, configs, platform interfaces and `Clock` work in every scope, `.server`
// included.
import k.db.Database
import k.http.HttpClient
import k.time.Clock

entity Hit {
    @Id let id: Int
}

repository HitRepository for Hit {}

config CounterConfig {
    @Env("COUNTER_LIMIT") let limit: Int = 10
}

@Scope(.server)
service Counters {
    @Inject let hits: HitRepository
    @Inject let database: Database
    @Inject let config: CounterConfig
    @Inject let http: HttpClient
    @Inject let clock: Clock
}
