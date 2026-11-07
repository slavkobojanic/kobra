/// A page whose load bumps the request's `Visits` twice; the components
/// render the same instance.
public page Counts {
    @Inject let doubler: Doubler

    @Load let total: Int = await doubler.bumpTwice()

    view {
        paragraph { "load counted {total}" }

        VisitBadge()
        VisitBadge()
    }

    loading {
    }
}
