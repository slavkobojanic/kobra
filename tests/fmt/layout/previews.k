public component Stepper {
    @Binding var value: Int
    view {
        Text("value")
    }
}
preview(value:Int=1,  disabled : Bool = false){
  Stepper(value: bind value)
      .disabled(disabled)
}
preview   "signed-in cart" (items: Int = 3) {
    use FakeCartService for CartService
      @Inject let cart: FakeCartService

    await cart.seed(count: items)
    CartPage()
}
preview "bare" { Stepper(value: bind value) }
