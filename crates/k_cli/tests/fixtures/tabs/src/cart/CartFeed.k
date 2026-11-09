/// Shows the notices of the other tabs: an effect over `messages`.
@Rendering(.client)
public component CartFeed {
    @Inject let notices: NoticeService

    @Effect
    func following() async {
        await notices.follow()
    }

    loading {
    }

    view {
        text { "Heard {notices.heard.count}" }
    }
}
