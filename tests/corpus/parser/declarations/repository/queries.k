// The repository from docs/design/19-persistence.md.
repository UserRepository for User {
    func byEmail(email: String) async -> User? {
        return users.first { user => user.email == email }
    }

    func activeSince(date: Date) async -> List<User> {
        return users
            .filter { user => user.lastSeen > date }
            .sortedBy { user => user.name }
            .limit(50)
    }
}
