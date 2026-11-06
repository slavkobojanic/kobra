// `Secret<T>` values can't reach client code.
component Pay {
    @Prop let key: Secret<String>

    view {
        text { "pay" }
    }
}
