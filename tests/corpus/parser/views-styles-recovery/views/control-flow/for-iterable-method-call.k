// A method call with arguments as the iterable, and a call as the key.
component T {
    view {
        for user in users.sorted(by: .name) key user.id.value {
            UserRow(user: user)
        }
    }
}
