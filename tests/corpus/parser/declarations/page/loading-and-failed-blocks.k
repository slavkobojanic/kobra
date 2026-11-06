// `loading`, `failed(pattern)` and `failed` without a binding are members.
page Orders {
    loading {
        Spinner()
    }

    failed(let error) {
        ErrorView(error: error)
    }

    view {
        OrderList()
    }
}

page Invoices {
    failed { "Something went wrong" }
}
