// Several misspelled block keywords in one component are each reported once, and the correct members between them still parse.
component T {
    lodaing {
        Spinner()
    }

    view {}

    faild(let e) {}

    @State var x: Int = 0

    metdata {
        title(name)
    }

    loading {}
}
