struct Point {
  let x: Int
        let y: Int

	func moved(by delta: Int) -> Point {
	    Point(x: x + delta,
	          y: y + delta)
	}
}

func total(items: List<Int>) -> Int {
var sum = 0
for item in items {
if item > 0 {
sum += item
}
}
return sum
}
