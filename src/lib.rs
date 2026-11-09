//! # luminess
//!
//! A template engine that compiles once, at startup, against typed views.
//!
//! - **Checked at startup.** Templates compile against a [`Schema`]. An unknown
//!   field, filter, or template is a [`CompileError`] with the template name,
//!   line, and column, never an empty string in production.
//! - **Escaped for where it goes.** The compiler tracks the HTML around every
//!   `{{ }}` and picks the escaper: HTML text, attribute value, URL,
//!   JavaScript, or CSS. Output where no escaper is safe (an unquoted
//!   attribute, a comment, a tag name, a regular expression) is refused at
//!   compile time. Unescaped output needs an explicit `| raw`.
//! - **No data conversion.** A render reads your values through the [`View`]
//!   trait, by field ids the compiler resolved, and writes straight into your
//!   `Vec<u8>`. No JSON, no maps, no copies of your data.
//! - **Blocks that render themselves.** A view can write its own markup, and
//!   when it fails its output is removed and its fallback written; the rest of
//!   the page still renders.
//! - **Live reload.** An [`Engine`] swaps in a newly compiled set while it
//!   serves; a set with errors is refused and the running one stays.
//! - **Zero dependencies.**
//!
//! ```
//! use luminess::Engine;
//!
//! struct Post { title: String, url: String }
//! struct Page { title: String, posts: Vec<Post> }
//!
//! luminess::view!(Post as "post" { title: text, url: text });
//! luminess::view!(Page as "page" { title: text, posts: list(object(Post)) });
//!
//! let engine = Engine::<Page>::builder()
//!     .template("home", r#"<h1>{{ page.title }}</h1>
//! <ul>{% for post in page.posts %}<li><a href="{{ post.url }}">{{ post.title }}</a></li>{% endfor %}</ul>"#)
//!     .build()?;
//!
//! let page = Page {
//!     title: "Tom & Jerry".into(),
//!     posts: vec![Post { title: "<script>".into(), url: "javascript:alert(1)".into() }],
//! };
//! let mut out = Vec::new();
//! engine.render("home", &page, &mut out)?;
//! assert_eq!(
//!     String::from_utf8(out)?,
//!     "<h1>Tom &amp; Jerry</h1>\n<ul><li><a href=\"#blocked\">&lt;script&gt;</a></li></ul>"
//! );
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! The template syntax is close to Jinja and Twig; the README describes it.

#![doc(html_root_url = "https://docs.rs/luminess")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(unused_must_use, unused_results)]
#![warn(clippy::pedantic)]
#![allow(
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::match_same_arms
)]

mod compile;
mod engine;
mod error;
pub mod escape;
mod html;
mod lex;
mod output;
mod parse;
mod render;
mod script;
mod view;
#[cfg(feature = "web-net")]
#[cfg_attr(docsrs, doc(cfg(feature = "web-net")))]
pub mod web;

#[doc(hidden)]
pub use view::__html;

pub use engine::{Builder, Engine, Templates};
pub use error::{CompileError, Diagnostic, RenderError, RenderErrorKind};
pub use output::{Failed, Output};
pub use render::Rendered;
pub use view::{
    Computed, FieldId, Kind, List, Schema, SchemaBuilder, SchemaRef, Shaped, ToValue, Value, View,
};
