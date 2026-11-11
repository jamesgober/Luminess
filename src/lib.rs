//! # luminess
//!
//! A template engine that compiles once, at startup, against your own Rust
//! types, and escapes every value for the place it is written.
//!
//! ## Start here
//!
//! Declare what templates may read, load a theme, render:
//!
//! ```no_run
//! use luminess::Engine;
//!
//! struct Page { title: String, body: String }
//! luminess::view!(Page as "page" { title: text, body: html });
//!
//! let engine = Engine::<Page>::from_dir("themes/default")?;   // every .html file
//! let html = engine.render("home", &Page { title: "Hi".into(), body: "<p>ok</p>".into() })?;
//! # Ok::<(), luminess::Error>(())
//! ```
//!
//! One template, compiled once:
//!
//! ```
//! # struct Page { title: String }
//! # luminess::view!(Page as "page" { title: text });
//! let title = luminess::Template::<Page>::new("<h1>{{ page.title }}</h1>")?;
//! assert_eq!(title.render(&Page { title: "Tom & Jerry".into() })?, "<h1>Tom &amp; Jerry</h1>");
//! # Ok::<(), luminess::Error>(())
//! ```
//!
//! One render, compiling the template on the spot (for scripts and tests):
//!
//! ```
//! # struct Page { title: String }
//! # luminess::view!(Page as "page" { title: text });
//! let html = luminess::render("<b>{{ page.title }}</b>", &Page { title: "x".into() })?;
//! assert_eq!(html, "<b>x</b>");
//! # Ok::<(), luminess::Error>(())
//! ```
//!
//! Every call returns [`Error`], so `?` works throughout.
//!
//! ## What it does
//!
//! - **Checked at startup.** An unknown field, filter, template, or block is
//!   an [`Error`] with the template name, line, and column, never an empty
//!   string in production.
//! - **Escaped for where it goes.** The compiler follows the HTML around each
//!   `{{ }}` and picks the escaper: HTML text, attribute value, URL,
//!   JavaScript, or CSS. Places where no escaper is safe are refused at
//!   startup. Unescaped output needs an explicit `| raw`.
//! - **No data conversion.** A render reads your values in place and writes
//!   straight into a buffer: no JSON, no maps, no copies.
//! - **Blocks that render themselves**, with a fallback when they fail; the
//!   rest of the page still renders.
//! - **Live reload.** [`Engine::reload_dir`] and [`Engine::replace`] swap in a
//!   new theme while serving; a theme with errors is refused and the running
//!   one stays.
//! - **Zero dependencies.**
//!
//! ## Going further
//!
//! - [`Engine::builder`]: templates from strings, output limits, your own
//!   filters, and [`Builder::check`] for CI.
//! - [`Engine::render_into`]: render into a buffer you reuse, and see which
//!   blocks fell back.
//! - [`View`], [`Value`], [`Schema`], and [`Output`]: views written by hand,
//!   schemas built at startup, and blocks that render their own markup.
//!
//! The template syntax is close to Jinja and Twig; `docs/SYNTAX.md` in the
//! repository describes it, and `docs/API.md` walks through the whole API.

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
pub use view::{__HtmlItems, __html, __html_list};

/// Runs every example in `docs/API.md` as a doctest.
#[cfg(doctest)]
#[doc = include_str!("../docs/API.md")]
struct ApiExamples;

pub use engine::{Builder, Engine, Template};
pub use error::{Diagnostic, Error, ErrorKind};
pub use output::{BlockError, Output};
pub use render::Rendered;
pub use view::{Answer, HasSchema, Html, Kind, List, Schema, SchemaBuilder, ToValue, Value, View};

/// Compiles `source` and renders it for `view`, in one call.
///
/// The template is compiled on every call, which costs tens of
/// microseconds: fine for a script, a test, or a one-off email, and the wrong
/// tool for a page served many times. For that, compile once with
/// [`Template::new`] or [`Engine::from_dir`] and render many times.
///
/// ```
/// struct Note { text: String }
/// luminess::view!(Note as "note" { text: text });
///
/// let html = luminess::render("<p>{{ note.text }}</p>", &Note { text: "a < b".into() })?;
/// assert_eq!(html, "<p>a &lt; b</p>");
/// # Ok::<(), luminess::Error>(())
/// ```
///
/// # Errors
///
/// [`ErrorKind::Compile`] with every problem in the template.
pub fn render<V: View + HasSchema>(source: &str, view: &V) -> Result<String, Error> {
    Template::<V>::new(source)?.render(view)
}
