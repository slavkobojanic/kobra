/// Links to `to` when it parses as a URL (an "open this link" page).
public page Go {
    @Param let to: String = ""

    view {
        main {
            if let target = Url.parse(to) {
                link(url: target) { "Continue" }
            } else {
                paragraph { "No link" }
            }
        }
    }
}
