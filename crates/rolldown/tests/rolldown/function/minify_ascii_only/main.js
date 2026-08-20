import assert from 'node:assert';

// String literals: BMP, astral, lone escapes.
export const strings = ['café', '日本語', '😀', ' ', 'a</script>b'];

// Identifiers, including a supplementary-plane one (must become \u{...}, not a surrogate pair).
let ñame = 1;
let 𠮷 = 2;
export function bump() {
  ñame++;
  𠮷++;
  return ñame + 𠮷;
}

// Destructuring assignment where the mangler renames the binding: the property key must be
// escaped as well.
export function pick(opts) {
  let ñame;
  ({ ñame } = opts);
  return ñame;
}

// Regular expressions: plain non-ASCII, an identity escape of a non-ASCII char, a class.
export const regexes = [/café/, /[\–\—]/g, /\’s/, /😀+/u];

// Untagged template literals: plain, NonEscapeCharacter, line continuation with U+2028.
export const templates = [`naïve ${1}`, `caf\é`, `a\ b`];

// Tagged templates keep their raw text (the tag observes it), so they are the one construct
// left as written; this one is ASCII so the whole file can be asserted 7-bit clean.
export const raw = String.raw`resume\n`;

// A top-level export keeps its name through the mangler: astral identifier -> \u{20BB7}.
export let 𠮷野家 = '𠮷';

assert.strictEqual(strings[0], 'café');
assert.strictEqual(strings[2].codePointAt(0), 0x1f600);
assert.strictEqual(bump(), 5);
assert.strictEqual(pick({ ñame: 7 }), 7);
assert.ok(regexes[1].test('–') && !regexes[1].test('u'));
assert.ok(regexes[2].test('’s'));
assert.ok(regexes[3].test('😀😀'));
assert.strictEqual(templates[1], 'café');
assert.strictEqual(templates[2], 'ab');
assert.strictEqual(raw, 'resume\\n');
assert.strictEqual(𠮷野家, '𠮷');
