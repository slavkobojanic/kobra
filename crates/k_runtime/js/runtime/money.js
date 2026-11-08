/**
 * @module k:runtime/money
 *
 * The exact `Decimal` and `Money` types of `k.core`, `Currency`, and
 * their JSON codecs (docs/compiler/runtime.md §12, "Money and Decimal").
 *
 * A feature chunk (runtime.md §2): only clients whose code builds, reads
 * or decodes a `Decimal`, `Money` or `Currency` ship it, and generated
 * code reaches it through its own namespace (`$k_money`), never `$k`.
 * The server loads it with the rest of the runtime. Panics use the
 * messages the language specifies ("currency mismatch: USD and EUR").
 */

import { checkInt, panic, unitCase } from "./values.js";
import { decodeFail, expectObject } from "./std.js";

// --------------------------------------------------------------- Decimal

/** `10n`, the base of `Decimal` scales. */
const TEN = 10n;

/**
 * `10^n` as a bigint.
 * @param {number} n a non-negative integer
 * @returns {bigint}
 */
function pow10(n) {
  return TEN ** BigInt(n);
}

/** An exact decimal: `units / 10^scale`. */
export class Decimal {
  /**
   * @param {bigint} units
   * @param {number} scale
   */
  constructor(units, scale) {
    this.units = units;
    this.scale = scale;
  }
  /**
   * Both operands' units at the larger of the two scales, and that scale.
   * @param {Decimal} o
   * @returns {[bigint, bigint, number]}
   */
  align(o) {
    const s = Math.max(this.scale, o.scale);
    return [this.units * pow10(s - this.scale), o.units * pow10(s - o.scale), s];
  }
  /**
   * The exact sum.
   * @param {Decimal} o
   * @returns {Decimal}
   */
  plus(o) {
    const [a, b, s] = this.align(o);
    return new Decimal(a + b, s);
  }
  /**
   * The exact difference.
   * @param {Decimal} o
   * @returns {Decimal}
   */
  minus(o) {
    const [a, b, s] = this.align(o);
    return new Decimal(a - b, s);
  }
  /**
   * The exact product.
   * @param {Decimal} o
   * @returns {Decimal}
   */
  times(o) {
    return new Decimal(this.units * o.units, this.scale + o.scale);
  }
  /**
   * The quotient with 18 extra digits of scale, truncated toward zero and normalized; panics on division by zero.
   * @param {Decimal} o
   * @returns {Decimal}
   */
  dividedBy(o) {
    if (o.units === 0n) panic("division by zero");
    const extra = 18;
    const n = this.units * pow10(o.scale + extra);
    return new Decimal(n / o.units, this.scale + extra).normalized();
  }
  /**
   * The same value with trailing zeros of the fraction removed.
   * @returns {Decimal} trailing zeros removed
   */
  normalized() {
    let u = this.units;
    let s = this.scale;
    while (s > 0 && u % TEN === 0n) {
      u /= TEN;
      s--;
    }
    return new Decimal(u, s);
  }
  /**
   * The negated value.
   * @returns {Decimal}
   */
  negate() {
    return new Decimal(-this.units, this.scale);
  }
  /**
   * Compares by value (`1.0` equals `1`): -1, 0 or 1.
   * @param {Decimal} o
   * @returns {number}
   */
  compare(o) {
    const [a, b] = this.align(o);
    return a < b ? -1 : a > b ? 1 : 0;
  }
  /**
   * Equality by value, whatever the scales.
   * @param {unknown} o
   * @returns {boolean}
   */
  equals(o) {
    return o instanceof Decimal && this.compare(o) === 0;
  }
  /**
   * A key equal for equal values, for `HashMap` and `HashSet`.
   * @returns {string}
   */
  hashKey() {
    const n = this.normalized();
    return "D" + n.units + "e" + n.scale;
  }
  /**
   * The plain decimal text, `-12.50`, with the value's own scale.
   * @returns {string}
   */
  toString() {
    const neg = this.units < 0n;
    let digits = (neg ? -this.units : this.units).toString();
    if (this.scale === 0) return (neg ? "-" : "") + digits;
    digits = digits.padStart(this.scale + 1, "0");
    return (neg ? "-" : "") + digits.slice(0, digits.length - this.scale) + "." + digits.slice(digits.length - this.scale);
  }
  /**
   * The JSON form: the decimal text.
   * @returns {string}
   */
  toJSON() {
    return this.toString();
  }
}

/**
 * Parses decimal text (`"24.50"`, `"-3"`); `null` if it isn't one.
 * @param {string} text
 * @returns {Decimal | null}
 */
export function parseDecimal(text) {
  const t = String(text).trim();
  let neg = false;
  let i = 0;
  if (t[0] === "-" || t[0] === "+") {
    neg = t[0] === "-";
    i = 1;
  }
  const body = t.slice(i);
  const dot = body.indexOf(".");
  const whole = dot < 0 ? body : body.slice(0, dot);
  const frac = dot < 0 ? "" : body.slice(dot + 1);
  const ok = (s) => {
    for (let j = 0; j < s.length; j++) if (s.charCodeAt(j) < 48 || s.charCodeAt(j) > 57) return false;
    return true;
  };
  if ((whole === "" && frac === "") || !ok(whole) || !ok(frac)) return null;
  const units = BigInt((whole || "0") + frac);
  return new Decimal(neg ? -units : units, frac.length);
}

/**
 * A `Decimal` from literal text.
 * @param {string} text
 * @returns {Decimal}
 */
export function decimal(text) {
  return parseDecimal(text) ?? new Decimal(0n, 0);
}

/**
 * `v` as a `Decimal`: itself, or the parse of its text (0 when that fails).
 * @param {unknown} v
 * @returns {Decimal}
 */
function toDecimal(v) {
  return v instanceof Decimal ? v : decimal(String(v));
}

/**
 * `a + b` on decimals.
 * @param {Decimal} a
 * @param {Decimal} b
 * @returns {Decimal}
 */
export function decimalPlus(a, b) {
  return toDecimal(a).plus(toDecimal(b));
}

/**
 * `a - b` on decimals.
 * @param {Decimal} a
 * @param {Decimal} b
 * @returns {Decimal}
 */
export function decimalMinus(a, b) {
  return toDecimal(a).minus(toDecimal(b));
}

/**
 * `a * b` on decimals.
 * @param {Decimal} a
 * @param {Decimal} b
 * @returns {Decimal}
 */
export function decimalTimes(a, b) {
  return toDecimal(a).times(toDecimal(b));
}

/**
 * `a / b` on decimals.
 * @param {Decimal} a
 * @param {Decimal} b
 * @returns {Decimal}
 */
export function decimalDividedBy(a, b) {
  return toDecimal(a).dividedBy(toDecimal(b));
}

// ------------------------------------------------------------------ Money

/** The ISO 4217 currencies, in the declaration order of `Currency`. */
const CURRENCY_CODES = ["USD", "EUR", "GBP", "JPY", "CHF"];
/** The minor-unit digits of each currency, in the order of `CURRENCY_CODES`. */
const CURRENCY_DIGITS = [2, 2, 2, 0, 2];

/** The `Currency` enum's cases (the runtime owns their representation). */
export const Currency = Object.freeze({
  usd: unitCase(0),
  eur: unitCase(1),
  gbp: unitCase(2),
  jpy: unitCase(3),
  chf: unitCase(4),
});

/** The `Currency` cases by tag. */
const CURRENCY_CASES = [Currency.usd, Currency.eur, Currency.gbp, Currency.jpy, Currency.chf];

/**
 * The upper-case ISO code of a currency.
 * @param {{$: number}} c
 * @returns {string}
 */
export function currencyCode(c) {
  return CURRENCY_CODES[c.$];
}

/**
 * The minor-unit digits of a currency.
 * @param {{$: number}} c
 * @returns {number}
 */
export function currencyDigits(c) {
  return CURRENCY_DIGITS[c.$];
}

/**
 * `Currency.parse(code:)`: the case for an upper-case code, or `null`.
 * @param {string} code
 * @returns {{$: number} | null}
 */
export function currencyParse(code) {
  const i = CURRENCY_CODES.indexOf(code);
  return i < 0 ? null : CURRENCY_CASES[i];
}

/** An exact amount of money: minor units (bigint) of a currency. */
export class Money {
  /**
   * @param {bigint} minor
   * @param {{$: number}} currency
   */
  constructor(minor, currency) {
    this.minor = minor;
    this.currency = currency;
  }
  /**
   * Panics unless `o` is in the same currency.
   * @param {Money} o
   */
  same(o) {
    if (o.currency !== this.currency) panic(`currency mismatch: ${currencyCode(this.currency)} and ${currencyCode(o.currency)}`);
  }
  /**
   * The sum; panics on a currency mismatch.
   * @param {Money} o
   * @returns {Money}
   */
  plus(o) {
    this.same(o);
    return new Money(this.minor + o.minor, this.currency);
  }
  /**
   * The difference; panics on a currency mismatch.
   * @param {Money} o
   * @returns {Money}
   */
  minus(o) {
    this.same(o);
    return new Money(this.minor - o.minor, this.currency);
  }
  /**
   * The amount multiplied by the integer `n`.
   * @param {number} n
   * @returns {Money}
   */
  times(n) {
    return new Money(this.minor * BigInt(n), this.currency);
  }
  /**
   * The negated amount.
   * @returns {Money}
   */
  negate() {
    return new Money(-this.minor, this.currency);
  }
  /**
   * The amount in minor units as an `Int`; panics outside the safe integer range.
   * @returns {number}
   */
  minorUnits() {
    return checkInt(Number(this.minor));
  }
  /**
   * The amount scaled by `rate`, rounded to the currency's digits by a
   * `RoundingRule` (halfEven, halfUp, down, up).
   * @param {Decimal} rate
   * @param {{$: number}} rule
   * @returns {Money}
   */
  multiplied(rate, rule) {
    const r = toDecimal(rate);
    const n = this.minor * r.units;
    const d = pow10(r.scale);
    let q = n / d;
    const rem = n % d;
    if (rem !== 0n) {
      const neg = n < 0n;
      const absRem = rem < 0n ? -rem : rem;
      const twice = absRem * 2n;
      let away;
      switch (rule ? rule.$ : 0) {
        case 1:
          away = twice >= d;
          break;
        case 2:
          away = false;
          break;
        case 3:
          away = true;
          break;
        default:
          away = twice > d || (twice === d && (q < 0n ? -q : q) % 2n === 1n);
      }
      if (away) q += neg ? -1n : 1n;
    }
    return new Money(q, this.currency);
  }
  /**
   * Splits into `parts` amounts that add up exactly; the first ones get the
   * remainder.
   * @param {number} parts
   * @returns {Money[]}
   */
  allocated(parts) {
    if (parts < 1) panic(`cannot allocate into ${parts} parts`);
    const n = BigInt(parts);
    const q = this.minor / n;
    let r = this.minor % n;
    const out = [];
    for (let i = 0; i < parts; i++) {
      let v = q;
      if (r > 0n) {
        v += 1n;
        r -= 1n;
      } else if (r < 0n) {
        v -= 1n;
        r += 1n;
      }
      out.push(new Money(v, this.currency));
    }
    return out;
  }
  /**
   * Compares amounts of the same currency: -1, 0 or 1; panics on a currency mismatch.
   * @param {Money} o
   * @returns {number}
   */
  compare(o) {
    this.same(o);
    return this.minor < o.minor ? -1 : this.minor > o.minor ? 1 : 0;
  }
  /**
   * Equality: the same currency and amount.
   * @param {unknown} o
   * @returns {boolean}
   */
  equals(o) {
    return o instanceof Money && o.currency === this.currency && o.minor === this.minor;
  }
  /**
   * A key equal for equal amounts, for `HashMap` and `HashSet`.
   * @returns {string}
   */
  hashKey() {
    return "M" + this.minor + currencyCode(this.currency);
  }
  /**
   * The amount as decimal text with the currency's digits, `12.50`.
   * @returns {string} the amount with the currency's digits
   */
  amount() {
    return new Decimal(this.minor, currencyDigits(this.currency)).toString();
  }
  /**
   * Formats the amount in a `NumberStyle` (`.currency` by default). The
   * amount goes to `Intl` as its exact decimal text, never through a
   * double, so no amount is rounded (34-money.md).
   * @param {{$: number}} [style] decimal 0, wholeNumber 1, percent 2, currency 3
   * @returns {string}
   */
  formatted(style) {
    const code = currencyCode(this.currency);
    const amount = this.amount();
    if (typeof Intl === "undefined") return `${amount} ${code}`;
    const d = currencyDigits(this.currency);
    const tag = style ? style.$ : 3;
    const o = tag === 3 ? { style: "currency", currency: code } : tag === 1 ? { maximumFractionDigits: 0 } : tag === 2 ? { style: "percent" } : { minimumFractionDigits: d, maximumFractionDigits: d };
    return new Intl.NumberFormat("en-US", o).format(amount);
  }
  /**
   * The amount and code, `12.50 USD`.
   * @returns {string}
   */
  toString() {
    return `${this.amount()} ${currencyCode(this.currency)}`;
  }
  /**
   * The JSON form, `{ amount, currency }`.
   * @returns {{amount: string, currency: string}}
   */
  toJSON() {
    return { amount: this.amount(), currency: currencyCode(this.currency) };
  }
}

/**
 * A money literal: `money("24.00", Currency.usd)`; panics for more digits
 * than the currency has.
 * @param {string} text
 * @param {{$: number}} currency
 * @returns {Money}
 */
export function money(text, currency) {
  const d = decimal(text);
  const digits = currencyDigits(currency);
  if (d.scale > digits) {
    const n = d.normalized();
    if (n.scale > digits) panic(`${text} has more decimal places than ${currencyCode(currency)} allows`);
    return new Money(n.units * pow10(digits - n.scale), currency);
  }
  return new Money(d.units * pow10(digits - d.scale), currency);
}

/**
 * `Money(minorUnits:currency:)`.
 * @param {number} minor
 * @param {{$: number}} currency
 * @returns {Money}
 */
export function moneyFromMinor(minor, currency) {
  return new Money(BigInt(minor), currency);
}

/**
 * `Money.zero(currency)`.
 * @param {{$: number}} currency
 * @returns {Money}
 */
export function moneyZero(currency) {
  return new Money(0n, currency);
}

/**
 * `sum(in:_:)`: the `Money` sum of `amount` over the elements.
 * @param {any[]} xs
 * @param {{$: number}} currency
 * @param {(v: any) => Money} amount
 * @returns {Money}
 */
export function sumMoney(xs, currency, amount) {
  let total = moneyZero(currency);
  for (const x of xs) total = total.plus(amount(x));
  return total;
}

// ----------------------------------------------------------------- codecs

/**
 * The codecs of `Decimal`, `Currency` and `Money`, by builtin codec name
 * (docs/design/28-rpc-wire-format.md); the other builtins are `codecs`.
 */
export const moneyCodecs = {
  decimal: {
    encode: (v) => v.toString(),
    decode: (j, p) => (typeof j === "string" && parseDecimal(j)) || decodeFail(p, "expected a decimal string"),
  },
  currency: {
    encode: (v) => currencyCode(v),
    decode: (j, p) => (typeof j === "string" && currencyParse(j)) || decodeFail(p, "expected a currency code"),
  },
  money: {
    encode: (v) => v.toJSON(),
    decode: (j, p) => {
      expectObject(j, p);
      const c = typeof j.currency === "string" ? currencyParse(j.currency) : null;
      if (!c) decodeFail(p + ".currency", "expected a currency code");
      if (typeof j.amount !== "string" || !parseDecimal(j.amount)) decodeFail(p + ".amount", "expected a decimal string");
      return money(j.amount, c);
    },
  },
};
