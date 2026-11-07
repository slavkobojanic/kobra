// `@Form` state on the client (docs/compiler/runtime.md §9,
// docs/design/15-forms-and-validation.md): raw text per field, parsing,
// constraints, cross-field rules, error wiring with `aria-invalid` and
// `aria-describedby`, and `handle(action)` with double-submit protection.
//
// runtime.md §9 fixes `createForm(fields, initial, validate)` but not the
// object it returns. These tests assume the shape below, which mirrors the
// K surface of `k.validation.Form`; the implementation must either provide
// it or update this header and the tests together:
//
//   form.field(name)          → { value(), set(raw), error(), isTouched(), touch(), attach(input, message?) }
//   form.isValid(), form.isSubmitting(), form.isTouched(), form.errors()  (reactive reads)
//   form.value()              → the parsed struct, only while the action runs
//   form.handle(action)       → (event?) => Promise<void>; calls action(parsedStruct)
//   validate(value, rules)    → rules.check(ok, fieldName, message), fieldName a string
import { assert, assertDeepEq, assertEq, test, tick } from "k:testing";
import { batch, createForm, renderEffect, root } from "k:runtime";

const FORMS = "forms (runtime.md §9)";

/** A `String` field: never fails to parse. */
function textField(name, constraints = []) {
  return { name, parse: (raw) => ({ ok: true, value: raw }), constraints };
}

/** An `Int` field with its `@Format` message. */
function intField(name, message = "Enter a whole number", constraints = []) {
  return {
    name,
    parse: (raw) => (/^-?[0-9]+$/.test(raw.trim()) ? { ok: true, value: Number(raw.trim()) } : { ok: false, message }),
    constraints,
  };
}

/** `@Length(min: n)`. */
const minLength = (n, message = `Use at least ${n} characters`) => (v) => (v.length >= n ? null : message);
/** `@Range(lo...hi)`. */
const range = (lo, hi, message = `Enter a number from ${lo} to ${hi}`) => (v) => (v >= lo && v <= hi ? null : message);

const noRules = () => {};

/** A deferred promise for actions that finish when the test says so. */
function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

/** The sign-up form of design 15 without the email field. */
function signup(initial = {}) {
  return root(() => createForm(
    [textField("name", [minLength(3)]), textField("password", [minLength(8)]), textField("confirmPassword"), intField("age", "Enter your age", [range(13, 120)])],
    initial,
    (value, rules) => rules.check(value.confirmPassword === value.password, "confirmPassword", "Passwords must match"),
  ));
}

/** Fills every field of `signup()` with valid text. */
function fillValid(form) {
  form.field("name").set("Ada");
  form.field("password").set("correct horse");
  form.field("confirmPassword").set("correct horse");
  form.field("age").set("36");
}

// ---------------------------------------------------------------- raw text

// Fields start from the initial raw text; fields not named start blank.
test("fields start with the initial raw text or blank", () => {
  const form = signup({ age: "18" });
  assertEq(form.field("age").value(), "18");
  assertEq(form.field("name").value(), "");
});

// A field keeps the text exactly as typed, even when it doesn't parse yet.
test("a field keeps unparsable text as typed", () => {
  const form = signup();
  form.field("age").set("3x");
  assertEq(form.field("age").value(), "3x");
  assertEq(form.field("age").error(), null, "nothing is reported before validation");
});

// A field's raw value is reactive, so bindings follow it.
test("a field's value is reactive", () => {
  const form = signup();
  const seen = [];
  root(() => renderEffect(() => seen.push(form.field("name").value())));
  batch(() => form.field("name").set("Ad"));
  assertDeepEq(seen, ["", "Ad"]);
});

// ---------------------------------------------------------------- validation

// A field that doesn't parse shows only its `@Format` message, and its
// constraints don't run.
test("a parse failure shows only the format message", async () => {
  let constraintRan = false;
  const form = root(() => createForm([intField("age", "Enter your age", [() => { constraintRan = true; return "never"; }])], {}, noRules));
  form.field("age").set("old");
  await form.handle(async () => {})();
  assertEq(form.field("age").error(), "Enter your age");
  assertEq(constraintRan, false);
});

// Constraints run on the parsed value: `36` passes `@Range(13...120)`, `7`
// doesn't.
test("constraints run on the parsed value", async () => {
  const form = signup();
  fillValid(form);
  form.field("age").set("7");
  await form.handle(async () => {})();
  assertEq(form.field("age").error(), "Enter a number from 13 to 120");
  assertEq(form.field("name").error(), null);
});

// Every failing field gets its message in one validation, not just the
// first one.
test("every failing field is reported at once", async () => {
  const form = signup();
  form.field("name").set("A");
  form.field("password").set("short");
  form.field("age").set("");
  await form.handle(async () => {})();
  assertEq(form.field("name").error(), "Use at least 3 characters");
  assertEq(form.field("password").error(), "Use at least 8 characters");
  assertEq(form.field("age").error(), "Enter your age");
  assertDeepEq(form.errors().map((e) => e.path), ["name", "password", "age"]);
});

// Cross-field rules (`validate(rules:)`) run after the fields parse and
// report under the field they name.
test("cross-field rules report under the named field", async () => {
  const form = signup();
  fillValid(form);
  form.field("confirmPassword").set("something else");
  await form.handle(async () => {})();
  assertEq(form.field("confirmPassword").error(), "Passwords must match");
  assertEq(form.field("password").error(), null);
});

// Cross-field rules don't run while a field fails to parse: the struct
// doesn't exist yet.
test("cross-field rules wait until every field parses", async () => {
  let ran = false;
  const form = root(() => createForm([intField("age")], {}, () => { ran = true; }));
  form.field("age").set("x");
  await form.handle(async () => {})();
  assertEq(ran, false);
});

// `isValid` is reactive and becomes true once the errors are fixed.
test("isValid follows the fields", async () => {
  const form = signup();
  const seen = [];
  root(() => renderEffect(() => seen.push(form.isValid())));
  await form.handle(async () => {})();
  assertEq(form.isValid(), false);
  fillValid(form);
  await form.handle(async () => {})();
  assertEq(form.isValid(), true);
  assertEq(seen[seen.length - 1], true);
});

// A field becomes touched on blur (`touch`), and the form once any field is
// edited.
test("touched flags follow blur and edits", () => {
  const form = signup();
  assertEq(form.isTouched(), false);
  assertEq(form.field("name").isTouched(), false);
  form.field("name").touch();
  assertEq(form.field("name").isTouched(), true);
  assertEq(form.field("age").isTouched(), false);
  form.field("age").set("1");
  assertEq(form.isTouched(), true);
});

// Fixed fields lose their error on the next validation.
test("a fixed field loses its error", async () => {
  const form = signup();
  fillValid(form);
  form.field("name").set("A");
  await form.handle(async () => {})();
  assertEq(form.field("name").error(), "Use at least 3 characters");
  form.field("name").set("Ada");
  await form.handle(async () => {})();
  assertEq(form.field("name").error(), null);
  assertDeepEq(form.errors(), []);
});

// ---------------------------------------------------------------- handle

// A valid form calls the action with the parsed struct: typed fields are
// values, not text.
test("handle calls the action with the parsed struct", async () => {
  const form = signup();
  fillValid(form);
  let received = null;
  await form.handle(async (value) => { received = value; })();
  assertDeepEq(received, { name: "Ada", password: "correct horse", confirmPassword: "correct horse", age: 36 });
});

// `form.value` is readable while the action runs, as the same parsed struct.
test("value is the parsed struct while the action runs", async () => {
  const form = signup();
  fillValid(form);
  let inside = null;
  await form.handle(async () => { inside = form.value(); })();
  assertEq(inside.age, 36);
});

// An invalid form never calls the action.
test("an invalid form doesn't call the action", async () => {
  const form = signup();
  let calls = 0;
  await form.handle(async () => { calls++; })();
  assertEq(calls, 0);
});

// While the action runs, `isSubmitting` is true and a second submit is
// refused: the action runs once.
test("handle refuses a double submit", async () => {
  const form = signup();
  fillValid(form);
  const gate = deferred();
  let calls = 0;
  const submit = form.handle(async () => { calls++; await gate.promise; });
  const first = submit();
  await tick();
  assertEq(form.isSubmitting(), true);
  await submit();
  assertEq(calls, 1);
  gate.resolve();
  await first;
  assertEq(form.isSubmitting(), false);
});

// After the action finished, the form can be submitted again.
test("the form can be submitted again after the action finished", async () => {
  const form = signup();
  fillValid(form);
  let calls = 0;
  const submit = form.handle(async () => { calls++; });
  await submit();
  await submit();
  assertEq(calls, 2);
});

// A `ValidationError` the action throws (a server-side failure passed on
// with `catch .rpcFailure(.invalid(let failure)) { throw failure }`) is shown
// under the fields it names, and the handler resolves instead of rejecting.
test("a ValidationError from the action is shown under its fields", async () => {
  const form = signup();
  fillValid(form);
  const outcome = await form.handle(async () => {
    throw { failures: [{ path: "name", message: "That name is taken" }] };
  })().then(() => "resolved", () => "rejected");
  assertEq(outcome, "resolved");
  assertEq(form.field("name").error(), "That name is taken");
  assertEq(form.isSubmitting(), false);
  assertEq(form.isValid(), false);
});

// The server names a failure by the rpc parameter's path (`request.email`
// for `signUp(request:)`, 28-rpc-wire-format.md): the form shows it under
// its own field, `email`, since both sides share the struct.
test("a server failure under the rpc parameter's name shows under its field", async () => {
  const form = signup();
  fillValid(form);
  await form.handle(async () => {
    throw { failures: [{ path: "request.name", message: "That name is taken" }] };
  })();
  assertEq(form.field("name").error(), "That name is taken");
});

// A failure for a path the form doesn't show (`address.city` on a form
// without it) is still in `errors`, so a summary can show it.
test("a server failure for an unknown path stays in errors", async () => {
  const form = signup();
  fillValid(form);
  await form.handle(async () => {
    throw { failures: [{ path: "address.city", message: "Unknown city" }] };
  })();
  assertDeepEq(form.errors(), [{ path: "address.city", message: "Unknown city" }]);
});

// The handler prevents the browser's own form submission, which would
// reload the page.
test("the submit handler prevents the default submission", async () => {
  const form = signup();
  const event = new globalThis.Event("submit", { bubbles: true });
  await form.handle(async () => {})(event);
  assert(event.defaultPrevented);
});

// ---------------------------------------------------------------- accessibility

/** An input and its error element, as the generated view creates them. */
function control() {
  const doc = globalThis.document;
  const input = doc.createElement("input");
  const message = doc.createElement("p");
  doc.body.appendChild(input);
  doc.body.appendChild(message);
  return { input, message };
}

// A failing field marks its input `aria-invalid="true"`, points
// `aria-describedby` at the error element (which gets an id) and shows the
// message there.
test("a failing field wires aria-invalid and aria-describedby", async () => {
  const form = signup();
  const { input, message } = control();
  root(() => form.field("name").attach(input, message));
  form.field("name").set("A");
  await form.handle(async () => {})();
  assertEq(input.getAttribute("aria-invalid"), "true");
  assert(message.id !== "", "the error element has an id");
  assertEq(input.getAttribute("aria-describedby"), message.id);
  assertEq(message.textContent, "Use at least 3 characters");
});

// A valid field is not marked invalid and its error element is empty.
test("a valid field is not marked invalid", async () => {
  const form = signup();
  const { input, message } = control();
  root(() => form.field("name").attach(input, message));
  fillValid(form);
  form.field("name").set("A");
  await form.handle(async () => {})();
  form.field("name").set("Ada");
  await form.handle(async () => {})();
  assertEq(input.hasAttribute("aria-invalid"), false);
  assertEq(message.textContent, "");
});

// Without an error element (a server-rendered control, whose markup has
// none), one is made right under the input when the first error shows, and
// numbers typed into a number field parse like their text.
test("an attached field without an error element gets one when it fails", async () => {
  const form = signup();
  const doc = globalThis.document;
  const input = doc.createElement("input");
  const next = doc.createElement("label");
  doc.body.appendChild(input);
  doc.body.appendChild(next);
  root(() => form.field("name").attach(input));
  assertEq(input.nextSibling, next, "nothing is added while the field is valid");
  form.field("name").set("A");
  await form.handle(async () => {})();
  const message = input.nextSibling;
  assertEq(message.localName, "p");
  assertEq(message.getAttribute("role"), "alert");
  assertEq(message.textContent, "Use at least 3 characters");
  assertEq(input.getAttribute("aria-describedby"), message.id);
  form.field("name").set("Ada");
  await form.handle(async () => {})();
  assertEq(message.textContent, "");
  assertEq(input.hasAttribute("aria-invalid"), false);
});

// Two forms on one page give their error elements distinct ids.
test("error element ids are unique across forms", async () => {
  const a = signup();
  const b = signup();
  const ca = control();
  const cb = control();
  root(() => {
    a.field("name").attach(ca.input, ca.message);
    b.field("name").attach(cb.input, cb.message);
  });
  await a.handle(async () => {})();
  await b.handle(async () => {})();
  assert(ca.message.id !== cb.message.id, `${ca.message.id} vs ${cb.message.id}`);
});

// Typing into an attached input updates the raw text.
test("typing into an attached input updates the field", () => {
  const form = signup();
  const { input, message } = control();
  root(() => form.field("age").attach(input, message));
  input.value = "4";
  input.dispatchEvent(new globalThis.Event("input", { bubbles: true }));
  assertEq(form.field("age").value(), "4");
});
