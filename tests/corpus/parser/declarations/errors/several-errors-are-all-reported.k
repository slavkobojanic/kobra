// One bad construct does not hide later errors: each declaration reports its own.
struct {}
enum E { case a, b }
func f() throws {}
var top = 1
component Ok {
    42
}
struct Fine {}
