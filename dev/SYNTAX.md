# Luminess template syntax (proposal)

Status: draft for review. The syntax may change until 0.5.0, when it freezes.

The syntax follows Jinja and Twig, so anyone who has written a Jinja, Twig,
Liquid, or Django template can read a Luminess template on sight. Where
Luminess differs, it is because templates compile against a typed view and are
checked at boot.

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
| inside a `<script>` element or an `on...` attribute | a JavaScript string literal with every quote, `<`, `>`, `&`, and line break escaped |
| inside a `<style>` element or a `style` attribute | CSS with every character other than letters and digits escaped |

These are compile errors: output in an unquoted attribute value, in a tag name,
in an attribute name, or inside an HTML comment; a `<script>` or `<style>`
element left open at the end of the template.

`| raw` writes a text value unescaped. It is allowed in HTML text only. Search a
theme for `| raw` to review every unescaped value.

## 7. Filters

| Filter | Effect |
|---|---|
| `default(v)` | `v` when the value is absent or empty |
| `upper`, `lower` | case conversion |
| `trim` | remove leading and trailing whitespace |
| `length` | the number of items in a list or characters in a text |
| `raw` | unescaped output (see section 6) |

Filters chain: `{{ post.title | trim | upper }}`. An application can register
its own filters; they are checked at compile time like fields.

## 8. Includes and layouts

`{% include "name" %}` inserts another template of the same set. It is resolved
and checked at boot, sees the same values as the place it is included from, and
may not include itself.

Layout inheritance (`{% extends "base" %}` with `{% block content %}...{% endblock %}`)
follows in 0.4.

## 9. Blocks that fail

A block that renders itself (a gallery, a product's stock) can fail at run time,
for example when the service behind it is down. The rest of the page still
renders:

- Without `try`, the failed block's output is removed and the block's own
  fallback is written (empty unless the block defines one). The failure is
  reported to the caller.
- With `{% try %} ... {% fallback %} ... {% endtry %}`, everything written
  since `try` is removed and the fallback branch is rendered instead.

## 10. Decisions to confirm

| Id | Question | Proposal |
|---|---|---|
| S-1 | Jinja-like syntax | yes: `{{ }}`, `{% %}`, `{# #}` |
| S-2 | End tags | `endif`, `endfor`, `endtry` (Jinja); not a bare `end` |
| S-3 | Unescaped marker | `| raw` (Twig), HTML text only |
| S-4 | Root name | the view's name, `page` for pages |
| S-5 | Unknown names | compile error at boot, never a blank at run time |
| S-6 | Unquoted attribute values with output | compile error ("quote this attribute") |
| S-7 | Failing blocks | the block's fallback by default; `try` / `fallback` for custom markup |
| S-8 | Method calls with arguments (`variations.options("size")`) | 0.6, checked against the view's declared methods |
