// Route templates interpolate parameters.
page ProductDetail at "/product/{id}/{tab}" {
    @Param let id: ProductId
    @Param let tab: ProductTab
}
