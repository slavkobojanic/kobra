/// The live price.
public page Prices at "/" {
    @Inject let feed: PriceFeed

    @Effect
    func follow() async {
        await feed.follow()
    }

    view {
        if let cents = feed.cents {
            paragraph { "BTC {cents}" }
                .role(.status)
        }

        if not feed.isLive {
            paragraph { "Reconnecting…" }
                .role(.alert)
        }
    }
}
