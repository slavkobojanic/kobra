/// Infinite scroll: the next page loads as the end of the list nears the
/// viewport.
public component Feed {
    @Inject let feed: FeedService

    @State var seen: Int = 0
    @State var left: Int = 0

    view {
        list {
            for post in feed.posts key post {
                item { post }
            }
        }

        if feed.hasMore {
            text { "Loading more posts" }
                .appearMargin(400px)
                .appearThreshold(50%)
                .on(appeared: { await feed.loadMore() })
        }

        text { "Footer" }
            .on(appeared: { seen += 1 })
            .on(disappeared: { left += 1 })

        text { "Footer seen {seen} left {left}" }
    }
}
