// `@Load` belongs to pages, layouts and components; services load in methods.
func count() async -> Int {
    return 1
}

service Stats {
    @Load let total: Int = await count()
}
