// `?.` skips every later link of its chain on nil, not just the next one: in
// `user?.id.name` the read of `.name` sits behind the same nil test as `.id`
// (docs/compiler/codegen.md §5). A chain of plain fields keeps JavaScript's own
// `?.`, which already short-circuits the rest.
struct AccountId {
    let value: String

    var name: String {
        return "account {value}"
    }
}

struct User {
    let id: AccountId
    let tags: List<String>
}

struct Session {
    let user: User?
}

func nameOf(session: Session) -> String? {
    return session.user?.id.name
}

func valueOf(session: Session) -> String? {
    return session.user?.id.value
}

func firstTag(session: Session) -> String? {
    return session.user?.tags[0]
}
