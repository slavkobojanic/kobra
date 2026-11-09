@Rendering(.client)
public component CartPanel {
    view {
        CartTotal()
    }

    loading {
        text { "Loading the cart" }
    }
}
