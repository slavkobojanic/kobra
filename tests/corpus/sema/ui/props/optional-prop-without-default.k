// An optional prop without a written default is still optional (it defaults to
// `nil`), so a call may omit it.
component Avatar {
    @Prop let name: String
    @Prop let initials: String?

    view {
        text { name }
    }
}

component Home {
    view {
        Avatar(name: "Ada")
        Avatar(name: "Ada")
            .initials("AL")
    }
}
