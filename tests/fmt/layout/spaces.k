func spaces(a:Int,b : Int)->Int{
    let list=[1,2 , 3]
    let map=[ "a" :1 ]
    let empty : Map<String, Int> =[:]
    let range=0 ..< a
    let closed = 1 ... b
    let neg = - a
    let both = not(a==b)and a!=b
    let optional : Int?=nil
    let fallback = optional??0
    let call = max( a , b )
    let generic : List<Int> = []
    let lambda = { ( x : Int )->Int=>x*2 }
    let short = { x=>x+1 }
    let member = list . count
    let chained = list?.first
    let index = list [0]
    let text = "a { b } c{ a }"
    var total=0
    total+=a
    total -= b
    return a%b
}
