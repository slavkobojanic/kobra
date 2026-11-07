// `image(source:description:)` is an `img` whose `description` is its `alt`; `.loading(.lazy)` is
// `loading` (05-views.md).
component Logo {
    @Prop let photo: Url

    view {
        image(source: photo, description: "The K logo")
            .loading(.lazy)
    }
}
