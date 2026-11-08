/// The public page, with links to the guarded ones.
public page Home at "/" {
    @Inject let members: Members

    view {
        heading(level: 1) { "Home" }

        if let user = members.user {
            paragraph { "Signed in as {user.id.name}" }
        } else {
            paragraph { "Signed out" }
        }

        link(to: .account) { "Account" }
        link(to: .admin) { "Admin" }
    }
}
