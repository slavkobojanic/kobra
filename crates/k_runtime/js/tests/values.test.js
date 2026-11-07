// Values: panics, checked Int arithmetic and structural equality
// (docs/compiler/codegen.md, "Value representation").
import { assert, assertEq, assertThrows, test } from "k:testing";
import { KPanic, checkInt, equals, intDiv, intRem, isPanic, panic, textOf, unitCase } from "k:runtime";

test("panic throws a KPanic that isPanic recognizes", () => {
  const e = assertThrows(() => panic("index 3 is out of range"), "index 3");
  assert(e instanceof KPanic);
  assert(isPanic(e));
  assertEq(isPanic(new Error("x")), false);
});

test("checkInt accepts safe integers and panics outside the range", () => {
  assertEq(checkInt(Number.MAX_SAFE_INTEGER), Number.MAX_SAFE_INTEGER);
  assertEq(checkInt(-Number.MAX_SAFE_INTEGER), -Number.MAX_SAFE_INTEGER);
  assertThrows(() => checkInt(Number.MAX_SAFE_INTEGER + 1), "overflow");
  assertThrows(() => checkInt(0.5), "overflow");
  assertThrows(() => checkInt(Infinity), "overflow");
});

test("intDiv truncates toward zero and panics on zero", () => {
  assertEq(intDiv(7, 2), 3);
  assertEq(intDiv(-7, 2), -3);
  assertEq(intDiv(7, -2), -3);
  assertThrows(() => intDiv(1, 0), "division by zero");
});

test("intRem has the dividend's sign and never returns -0", () => {
  assertEq(intRem(7, 3), 1);
  assertEq(intRem(-7, 3), -1);
  assertEq(Object.is(intRem(-6, 3), 0), true);
  assertThrows(() => intRem(1, 0), "division by zero");
});

test("unit cases are frozen singletons", () => {
  const c = unitCase(2);
  assertEq(c.$, 2);
  assert(Object.isFrozen(c));
});

test("equals compares structurally", () => {
  assert(equals({ a: 1, b: [1, 2] }, { a: 1, b: [1, 2] }));
  assert(!equals({ a: 1 }, { a: 1, b: 2 }));
  assert(equals(new Map([[1, { x: 1 }]]), new Map([[1, { x: 1 }]])));
  assert(!equals(new Map([[1, 1]]), new Map([[2, 1]])));
  assert(equals(new Set([1, 2]), new Set([2, 1])));
  assert(equals(null, null));
  assert(!equals(null, 0));
  assert(!equals([1], { 0: 1 }));
  assert(equals(NaN, NaN));
});

test("textOf formats strings, numbers, booleans and nil", () => {
  assertEq(textOf("a"), "a");
  assertEq(textOf(42), "42");
  assertEq(textOf(true), "true");
  assertEq(textOf(null), "nil");
});

// A payload-free case is one value per tag, whichever copy of its enum
// built it: in a `k test` run the browser's and the server's modules both
// define the enum, and a case read back from the server (`order.status`)
// must be `===` the test's `.awaitingPayment`, as generated code compares
// payload-free cases by identity.
test("payload-free cases with the same tag are the same value across copies of an enum", () => {
  const browser = { awaitingPayment: unitCase(0), paid: unitCase(1) };
  const server = { awaitingPayment: unitCase(0), paid: unitCase(1) };
  assert(browser.awaitingPayment === server.awaitingPayment);
  assert(browser.paid !== server.awaitingPayment);
  assert(Object.isFrozen(server.paid));
});
