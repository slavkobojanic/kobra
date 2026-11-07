// Static text next to a dynamic text stays in the template; only the dynamic part is bound.
component Total {
    @Prop let count: Int

    view {
        paragraph {
            "Items: "

            text { "{count}" }
        }
    }
}
