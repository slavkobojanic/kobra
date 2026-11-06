// `@Prop` belongs on component fields only. On a service field it is K3002, and
// its mutability is not checked as well: `var` here gets no K3001.
service Cart {
    @Prop var total: Int = 0
    @State var count: Int = 0
}
