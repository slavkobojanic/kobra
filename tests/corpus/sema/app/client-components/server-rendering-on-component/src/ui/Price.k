@Rendering(.server)
public component Price {
    @Prop let cents: Int

    view {
        text { "{cents}" }
    }
}
