test Feed {
    @Inject let feed: FeedService

    test "nothing appears unless the test says so" {
        let screen = render(Feed())
        await screen.settle()

        expect feed.loads == 0
        expect screen.text("Footer seen 0 left 0").isVisible
    }

    test "the end of the list appearing loads the next page" {
        let screen = render(Feed())

        screen.text("Loading more posts").appear()
        await screen.settle()

        expect feed.loads == 1
        expect screen.items().count == 4
    }

    test "appeared fires on each entry, disappeared on each exit" {
        let screen = render(Feed())

        screen.text("Footer").appear()
        screen.text("Footer").appear()
        screen.text("Footer").disappear()
        screen.text("Footer").appear()

        expect screen.text("Footer seen 2 left 1").isVisible
    }

    test "disappearing before appearing fires nothing" {
        let screen = render(Feed())

        screen.text("Footer").disappear()

        expect screen.text("Footer seen 0 left 0").isVisible
    }

    test "the loader goes away with the last page" {
        let screen = render(Feed())

        screen.text("Loading more posts").appear()
        await screen.settle()
        screen.text("Loading more posts").disappear()
        screen.text("Loading more posts").appear()
        await screen.settle()

        expect feed.loads == 2
        expect not screen.text("Loading more posts").exists
        expect screen.items().count == 6
    }
}
