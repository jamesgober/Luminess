# Template syntax

The syntax follows Jinja and Twig. Where Luminess differs, it is because
templates compile against a typed view and are checked when the program
starts.

## 1. Summary

```text
{{ page.title }}                       output, escaped for where it sits (text, attribute, URL, script, style)
{{ page.body }}                        an html field is trusted markup and is written as is in HTML text
{{ page.note | raw }}                  explicit unescaped output; the only way, easy to grep
{{ page.subtitle | default("None") }}  filters with a pipe
{% if page.featured %} ... {% elif page.draft %} ... {% else %} ... {% endif %}
{% for post in page.posts %} {{ loop.index }}. {{ post.title }} {% else %} No posts. {% endfor %}
{% include "partials/nav" %}           resolved and checked at boot
{% try %} {{ page.stock }} {% fallback %} Stock unknown {% endtry %}
{# a comment #}   {% raw %}{{ not parsed }}{% endraw %}   {{- trims whitespace -}}
```

Unknown fields, unknown filters, unknown templates, and output in a place that
cannot be escaped safely are compile errors at boot, with the template name,
line, and column. Nothing fails as a silent blank at request time.

## 2. Delimiters

| Form | Meaning |
|---|---|
| `{{ expr }}` | output the value of `expr` |
| `{% tag ... %}` | a statement: `if`, `for`, `include`, `try`, `raw` and their closing tags |
| `{# ... #}` | a comment; never written to the output |
| `{{-`, `-}}`, `{%-`, `-%}` | remove the whitespace before or after the tag, including newlines |

Everything else is literal text and is copied to the output unchanged.

## 3. Values and paths

A template sees one root value whose name comes from its view: a page view is
`page`. Fields are reached with dots: `page.author.name`. Inside a loop, the
loop variable is a root of its own: `post.title`.

Each field has a declared kind, which the compiler checks:

| Kind | In a template |
|---|---|
| text | written escaped for the context |
| html | trusted markup (sanitized before it reached the view); written as is in HTML text, escaped elsewhere |
| int, float | written as a number |
| bool | used in conditions; writing one outputs `true` or `false` |
| object | has fields of its own; `{{ page.author }}` is an error unless the object renders itself |
| list | used in `for`; `| length` gives its size |
| block | an object that renders its own markup: `{{ page.gallery }}` |

A field can be absent at run time (an optional subtitle). An absent value writes
nothing and is false in a condition. `| default("...")` supplies a value.

## 4. Conditions

```text
{% if page.featured %}...{% endif %}
{% if not page.posts %}...{% endif %}
{% if page.count > 10 and page.public %}...{% endif %}
{% if page.status == "draft" %}...{% endif %}
```

Truthiness: false, absent, zero, the empty string, and the empty list are false;
everything else is true. Operators: `not`, `and`, `or`, `==`, `!=`, `<`, `<=`,
`>`, `>=`, and parentheses. Comparison operands are fields or literals (strings
in double or single quotes, integers, decimals, `true`, `false`).

## 5. Loops

```text
{% for post in page.posts %}
  <li class="{% if loop.first %}first{% endif %}">{{ loop.index }}. {{ post.title }}</li>
{% else %}
  <li>No posts yet.</li>
{% endfor %}
```

`loop.index` (from 1), `loop.index0` (from 0), `loop.first`, `loop.last`, and
`loop.length` are available in the body. The `else` branch runs when the list
is empty. Nested loops each have their own `loop`; the outer one is reached
through its variable only.

## 6. Escaping

The compiler tracks where each `{{ }}` sits in the HTML around it and picks the
escaper at compile time. There is nothing to choose in the template.

| Where | What is written |
|---|---|
| HTML text, `<title>`, `<textarea>` | `& < > " '` as entities |
| a quoted attribute value | the same, so the value cannot leave the quotes |
| a URL attribute (`href`, `src`, `action`, ...) | at the start: only `http`, `https`, `mailto`, `tel`, or relative URLs pass, others become `#blocked`; then percent-encoded |
| JavaScript code (a `<script>` element or an `on...` attribute) | a complete literal: text as a quoted string, numbers, `true`/`false`, absent as `null` |
| inside a JavaScript string or template literal | the content, with quotes, backslashes, `<`, `>`, `&`, `$`, braces, and line breaks escaped |
| a CSS value (a `<style>` element or a `style` attribute) | the value as is when it is only letters, digits, spaces, `# % . , - _ + * /`, and the functions `rgb rgba hsl hsla calc min max clamp var`; `blocked` otherwise |
| inside a CSS string | every character other than letters and digits as a CSS hex escape |

These are compile errors: output in an unquoted attribute value, in a tag name,
in an attribute name, inside an HTML comment, `srcdoc`, a JavaScript comment
or regular expression, a CSS comment, right after `$` in a template literal,
or in an `on...` or `style` attribute that uses character references; and a
template that ends inside a tag, attribute, comment, script, or style.

`| raw` writes a text value unescaped. It is allowed in HTML text only. Search a
theme for `| raw` to review every unescaped value.

## 7. Filters

| Filter | Effect |
|---|---|
| `default(v)` | `v` when the value is absent or empty |
| `upper`, `lower` | case conversion |
| `trim` | remove leading and trailing whitespace |
| `length` | the number of items in a list or characters in a text |
| `truncate(n)` | the first `n` characters, then `...` |
| `join(sep)` | a list's items with `sep` between them |
| `first`, `last` | a list's first or last item |
| `replace(from, to)` | text with every `from` replaced |
| `date(format)` | a Unix time (seconds, UTC): `%Y %m %d %H %M %S %b %B %%` |
| `number` | digits in groups of three: `1,234,567` |
| `raw` | unescaped output (see section 6) |
| `fallback("text")` | for a block that renders itself: the text written when it fails |

Filters chain: `{{ post.title | trim | upper }}`. An application can register
its own filters; they are checked at compile time like fields.

Methods: a schema may declare methods with typed parameters, called with
parentheses: `{{ variations.options("size") }}`, `{{ variations.cheapest() }}`.
Arguments are checked at compile time.

Local values: `{% set name = expr %}` names a value until the end of the block
it is in (the body of an `if`, `for`, `block`, or the template).

## 8. Includes and layouts

`{% include "name" %}` inserts another template of the same set. It is resolved
and checked at boot, sees the same values as the place it is included from, and
may not include itself.

`{% include "name" with value %}` gives the partial one value as its root,
named by the partial's own schema; the partial is checked against that value.

Layouts: a base template marks replaceable parts with
`{% block name %}...{% endblock %}` (or `{% endblock name %}`). A template that
starts with `{% extends "base" %}` holds only blocks; each must exist in the
base chain. `{{ super() }}` inside a block writes the parent's version.

## 9. Blocks that fail

A block that renders itself (a gallery, a product's stock) can fail at run time,
for example when the service behind it is down. The rest of the page still
renders:

- Without `try`, the failed block's output is removed and the block's own
  fallback is written (empty unless the block defines one). The failure is
  reported to the caller.
- With `{% try %} ... {% fallback %} ... {% endtry %}`, everything written
  since `try` is removed and the fallback branch is rendered instead.
