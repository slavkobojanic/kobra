// An error in one component does not hide an error in the next component of the same file.
component A {
    veiw {}
}

component B {
    view {
        button { "x" }
            .disabled(!ok)
    }
}
