// `@Env` belongs to config fields only.
struct Settings {
    @Env("PORT") let port: Int
}
