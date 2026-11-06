// `public` can't make a member more visible than its type, so it is redundant in a package-private type too.
struct Helper {
    public let id: Int
}
