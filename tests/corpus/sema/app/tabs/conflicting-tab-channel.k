// A `TabChannel<T>` pairs a name with its message type (45-web-platform.md, "Messages between tabs"):
// channels with the same name are the same channel, so two named alike must carry the same message
// type, or one tab would post a type another decodes as a different one. A channel named like an
// earlier one with another message type is K2408 at its name literal, with a label at the earlier
// one; redeclaring one with the same type is fine.
import k.device.TabChannel

enum CartChange implements Codable {
    case added(id: Int)
    case cleared
}

let cartChanges: TabChannel<CartChange> = TabChannel(name: "cart")
let sameChanges: TabChannel<CartChange> = TabChannel(name: "cart")
let cartCounts: TabChannel<Int> = TabChannel(name: "cart")
let themeChanges: TabChannel<String> = TabChannel(name: "theme")
