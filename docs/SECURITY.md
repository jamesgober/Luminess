# Security model

Luminess is built to stop cross-site scripting (XSS) by construction: a
template cannot write a value into a page without the escaping its place
needs, and places where no escaping is safe are refused before the
application serves a single page.

## Who is trusted

| Input | Trusted? | Why |
|---|---|---|
| Template source | yes | written by the site's developers or theme authors, compiled at startup |
| Schemas, views, application filters | yes | the application's own Rust code |
| `html` fields (`Value::Html`) | yes | the application vouches that it sanitized them before they reached the view |
| Every other value | **no** | it may hold anything a visitor typed |

A value marked `| raw` is written as is. `| raw` is allowed only in HTML
text, so `grep '| raw'` lists every place a theme writes unescaped output
apart from fields, methods, and filters the schema declares `html`. A value
is written as markup only where its declared kind is `html`: one declared
text is escaped even when the view or filter answers `Value::Html`.

## How the escaper is chosen

The compiler follows the HTML around each `{{ }}` with a small tokenizer
(text, tags, attribute names and values, comments with every way the HTML
parser ends one, declarations, and the raw-text and RCDATA elements
`script`, `style`, `title`, `textarea`, `xmp`, `iframe`, `noembed`,
`noframes`, `noscript`, and `plaintext`), and inside scripts and styles a
JavaScript and a CSS tracker. The JavaScript tracker reads `<!--` and, at the
start of a line, `-->` as line comments, as browsers do, and a string that
reaches a line break as broken. The escaper is fixed per output at compile
time:

| Place | Guarantee |
|---|---|
| HTML text | the value cannot open a tag, an entity, or a comment: `& < > " '` become entities |
| quoted attribute value | the value cannot leave the quotes; the same five characters become entities |
| `<title>`, `<textarea>`, and the other raw-text elements | as HTML text; the element cannot be closed |
| start of a URL attribute (only whitespace before it) | only `http`, `https`, `mailto`, `tel`, and relative URLs pass (leading spaces, control characters, and tabs or line breaks inside the scheme are seen through, as browsers do); anything else becomes `#blocked`; then bytes a URL does not allow are percent-encoded and `&` becomes `&amp;` |
| a URL whose scheme may still be open (after other text or output, before the first `/`, `?`, or `#`) | as later in a URL, and `:` is percent-encoded too, so the value cannot complete a scheme |
| later in a URL, before `?` | percent-encoded where a URL does not allow the byte; a stray `%` becomes `%25` |
| after `?` or `#` in a URL | every byte other than letters, digits, and `- . _ ~` is percent-encoded |
| JavaScript code | a complete literal: a double-quoted string, a number (negative numbers spaced so they cannot join an operator), `true`, `false`, or `null` |
| a JavaScript string or template literal | its content, with quotes, backslashes, `/`, `<`, `>`, `&`, `$`, braces, `=`, line breaks, control characters, U+2028, and U+2029 escaped: the string, the script element, and a `${` cannot be opened or closed |
| CSS code | the value as is when it is letters, digits, spaces, `# % . , - _ + * /`, and the functions `rgb rgba hsl hsla calc min max clamp var`; otherwise `blocked` |
| a CSS string | every character other than letters and digits as a CSS hex escape |

In `on...` and `style` attributes the script or style text is HTML-decoded by
the browser before it runs, so an attribute whose literal text uses a
character reference (`&quot;`, `&#39;`, ...) is not followed: output in it is
refused, with a message to move the code to a `<script>` or `<style>`
element.

## Refused at compile time

Output in an unquoted attribute value, a tag name, an attribute name, an HTML
comment, a `<!...>` declaration, `srcdoc`, an SVG animation value (`values`,
`to`, `from`, `by` on `animate`, `set`, `animateMotion`, `animateTransform`,
which can set an `href`), right after `<` in a raw-text element other than a
script, a script the HTML parser reads as double-escaped (after `<!--` and
`<script`), a JavaScript comment or regular
expression, right after `/` or a backslash in JavaScript or CSS, right after
`$` in a template literal, and in a CSS comment. A literal `:` or `&` after an
output, before a URL's first `/`, `?`, or `#`, is refused too: the output
could supply a scheme that the template's text completes. Branches of an `if`,
the body and `else` of a `for`, and the body and fallback of a `try` must end
in the same place (two places in one URL attribute join to what is safe for
both). A template must not end inside a tag, attribute, comment, script, or
style.

A template added with `Builder::template_with` is compiled against its own
schema. It can be rendered by name only when that schema reads the engine's
view type the same way (the same fields at the same positions); otherwise it
is only for `include ... with`, and rendering it by name finds no template.

## Known limits

These are accepted, documented trade-offs, not oversights:

- **Regular expressions after `)`.** Whether `/` after `)` starts a regular
  expression or a division cannot be told without parsing JavaScript; it is
  read as a division (as Go's `html/template` does). A regular expression
  right after `)` with a quote inside it (`if (a) /'/.test(s)`) would mislead
  the tracker. Write such code in a `.js` file, or wrap the expression in
  parentheses. After `++` and `--` the tracker reads `/` as it read it before
  them: a division after a postfix operator, a regular expression after a
  prefix one.
- **`noscript`** is read as raw text, as browsers do with scripting on. With
  scripting off its content is parsed as HTML, where no script runs.
- **`<meta http-equiv="refresh" content="...">`.** `content` is escaped as a
  plain attribute; a URL in it is not checked. Never put visitor input there.
- **`srcset`.** The scheme is checked only at the start of the value; image
  candidates after the first are normalized but not checked. Browsers do not
  run script from image URLs.
- **A valid URL can still load something.** The scheme check stops
  `javascript:` and its kin; it cannot know whether an `https` URL is one the
  site meant. In `<script src>`, `<iframe src>`, `<link href>`,
  `<base href>`, `<object data>`, and CSS `@import` or `url()`, a visitor's
  `https` URL is as dangerous as script. Never put visitor input there.
- **Template literals** nested more than 4 `${` deep are not followed; output
  inside them is refused.
- **Trusted markup is trusted.** `html` fields and `| raw` are written as
  they are. Sanitize HTML before it becomes a `Value::Html`.

## Denial of service

Templates are compiled once, at startup, under limits: statements and
expressions nest at most 64 deep (each `and`, `or`, and filter in a chain
counting as a level, so compiling never recurses deeper than that), includes
and `extends` chains at most 8, one template compiles to at most about a
million nodes and 16 MiB of text with its includes inlined, and a source is
at most 1 MiB (`Builder::max_template_size`). Compile time is linear in the
size of the compiled template. A render's output can be capped with
`Builder::max_output`. Rendering never recurses deeper than the compiled
expression nesting, and every loop runs over a list the view returns.

## Reporting a problem

Open an issue at <https://github.com/jamesgober/Luminess/issues>, or for
anything sensitive, write to the address in the crate's `authors` field.
