// Injected services, derived values and effects with and without arguments.
component Profile {
    @Inject let users: UserService
    @Prop let userId: UserId
    @Derived let user: User? = users.find(userId)

    @Effect(on: [userId])
    func reload() {
        users.load(userId)
    }

    @Effect
    func applyColorScheme() {
        Theme.use(.dark)
    }
}
