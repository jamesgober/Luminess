//! web-net integration (the `web-net` feature): an [`Engine`] is a
//! [`web_net::Render`], and a [`Theme`] adds error pages.
//!
//! The engine's view type is the application's own: web-net hands a page
//! handler's `&V` straight to the compiled templates, and nothing is
//! converted. Template names are resolved when the application is built, so
//! a route naming a template the theme does not have stops it from
//! starting. The ids web-net keeps stay valid across [`Engine::reload`].
//!
//! ```
//! use luminess::{Engine, web::{ErrorView, Theme}};
//! use web_net::Render;
//!
//! struct Page { title: String }
//! luminess::view!(Page as "page" { title: text });
//!
//! let pages = Engine::<Page>::builder().template("home", "<h1>{{ page.title }}</h1>").build()?;
//! let errors = Engine::<ErrorView>::builder()
//!     .template("error", "<h1>{{ error.status }} {{ error.title }}</h1>")
//!     .build()?;
//! let theme = Theme::new(pages).with_error_pages(errors);
//!
//! // What web-net does: resolve at build time, render per request.
//! let home = theme.template("home").expect("the theme has it");
//! let mut buf = Vec::new();
//! let mut out = web_net::Output::new(&mut buf, 1 << 20);
//! theme.render(home, &Page { title: "Hi".into() }, &mut out)?;
//! assert_eq!(buf, b"<h1>Hi</h1>");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use web_net::{ErrorPage, Output, Render, TemplateId};

use crate::{Engine, RenderErrorKind, View};

/// The values an error page template reads, as `error`: `status`, `title`,
/// `message`, and `reference` (what the visitor quotes to support).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ErrorView {
    /// The status (`404`).
    pub status: u16,
    /// Its generic title (`Not Found`).
    pub title: String,
    /// Its generic message.
    pub message: String,
    /// The request's reference.
    pub reference: String,
}

crate::view!(ErrorView as "error" { status: int, title: text, message: text, reference: text });

impl ErrorView {
    fn from_page(page: &ErrorPage<'_>) -> Self {
        Self {
            status: page.status,
            title: page.title.to_owned(),
            message: page.message.to_owned(),
            reference: page.reference.to_owned(),
        }
    }
}

fn convert(e: &crate::RenderError) -> web_net::RenderError {
    let code = match e.kind() {
        RenderErrorKind::NotFound => "luminess.not_found",
        RenderErrorKind::TooLarge => "luminess.too_large",
    };
    web_net::RenderError::new(code).with_detail(e.to_string())
}

impl<V: View + 'static> Render for Engine<V> {
    type View<'a> = V;

    fn template(&self, name: &str) -> Option<TemplateId> {
        self.register(name).map(TemplateId)
    }

    fn render(
        &self,
        template: TemplateId,
        view: &V,
        out: &mut Output<'_>,
    ) -> Result<(), web_net::RenderError> {
        // A block that fails is not an error: its fallback is written and
        // the page goes out.
        self.render_id(template.0, view, out.buffer())
            .map(|_| ())
            .map_err(|e| convert(&e))
    }
}

/// Page templates plus error page templates: what an application hands
/// web-net as its engine.
///
/// Error pages come from the error engine's `error/<status>` template
/// (`error/404`), else its `error` template, else web-net's built-in page.
#[derive(Clone, Debug)]
pub struct Theme<V> {
    pages: Engine<V>,
    errors: Option<Engine<ErrorView>>,
}

impl<V: View + 'static> Theme<V> {
    /// A theme of `pages`, with web-net's built-in error pages.
    #[must_use]
    pub fn new(pages: Engine<V>) -> Self {
        Self {
            pages,
            errors: None,
        }
    }

    /// Uses `errors` for error pages.
    #[must_use]
    pub fn with_error_pages(mut self, errors: Engine<ErrorView>) -> Self {
        self.errors = Some(errors);
        self
    }

    /// The page engine, for reloading.
    #[must_use]
    pub fn pages(&self) -> &Engine<V> {
        &self.pages
    }

    /// The error page engine, if any, for reloading.
    #[must_use]
    pub fn error_pages(&self) -> Option<&Engine<ErrorView>> {
        self.errors.as_ref()
    }
}

impl<V: View + 'static> Render for Theme<V> {
    type View<'a> = V;

    fn template(&self, name: &str) -> Option<TemplateId> {
        Render::template(&self.pages, name)
    }

    fn render(
        &self,
        template: TemplateId,
        view: &V,
        out: &mut Output<'_>,
    ) -> Result<(), web_net::RenderError> {
        Render::render(&self.pages, template, view, out)
    }

    fn error_page(
        &self,
        page: &ErrorPage<'_>,
        out: &mut Output<'_>,
    ) -> Result<bool, web_net::RenderError> {
        let Some(errors) = &self.errors else {
            return Ok(false);
        };
        let view = ErrorView::from_page(page);
        let buf = out.buffer();
        let start = buf.len();
        for name in [format!("error/{}", page.status), "error".to_owned()] {
            match errors.render(&name, &view, buf) {
                Ok(_) => return Ok(true),
                Err(e) if e.kind() == RenderErrorKind::NotFound => {}
                Err(e) => {
                    buf.truncate(start);
                    return Err(convert(&e));
                }
            }
        }
        Ok(false)
    }
}
