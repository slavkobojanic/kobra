// `Database` is injectable only into server-side services.
import k.db.Database

component Health {
    @Inject let database: Database

    view {
        text { "health" }
    }
}
