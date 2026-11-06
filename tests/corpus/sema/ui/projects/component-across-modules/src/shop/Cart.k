// Props, bindings and events of an imported component are checked across
// modules: chained options and events resolve to the other module's members, a
// missing `bind` is K3016 and an unknown option is K3103.
import ui.Stepper

public component Cart {
    @State var quantity: Int = 1
    @State var last: Int = 0

    view {
        Stepper(value: bind quantity)
            .step(2)
           //^^^^ resolves: member ui.Stepper.step
            .on(changed: { v => last = v })
            //  ^^^^^^^ resolves: event ui.Stepper.changed
        Stepper(value: quantity)
        Stepper(value: bind quantity)
            .stp(3)
    }
}
