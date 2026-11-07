// Percent and Angle literals keep their number.
func main() -> List<String> {
    let half = 50%
    let turn = 45deg
    return ["{half}", "{turn.degrees}", "{Angle(degrees: 90.0).degrees}"]
}
