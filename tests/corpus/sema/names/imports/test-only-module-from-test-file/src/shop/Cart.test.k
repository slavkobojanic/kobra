// Test files may import test-only modules.
import testing.Samples

test "samples are small" {
    expect Cart(size: Samples.size).size == 3
    //                ^^^^^^^ resolves: decl testing.Samples
}
