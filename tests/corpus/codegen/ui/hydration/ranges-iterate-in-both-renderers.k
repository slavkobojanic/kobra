// A `for` over a range in a view: a `Range` isn't a JavaScript iterable, so both the client's `each`
// and the server renderer's `for … of` iterate its values (`$k.rangeValues`), for closed and
// half-open ranges alike. The server renderer used to call `for (const i of $k.closedRange(1, 40))`
// and fail with "not iterable".
component Pages {
    @Prop let last: Int

    view {
        list {
            for i in 1...40 key i {
                item { "{i}" }
            }
        }

        list {
            for i in 0..<last key i {
                item { "{i}" }
            }
        }
    }
}
