// A test-only module may import k.test and other test-only modules.
import k.test.Key
import testing.support.Seed

public struct Fixtures {
    let key: Key
    //       ^^^ resolves: decl k.test.Key
    let seed: Seed
    //        ^^^^ resolves: decl testing.support.Seed
}
