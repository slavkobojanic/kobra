// `@Effect` and `@Emits` apply only in components, pages, layouts and services.
struct Logger {
    let prefix: String

    @Effect
    func log() {
    }
}
