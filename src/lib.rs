//! # Luminess - Blazing Fast Template Engine
//!
//! Luminess is a high-performance template engine designed for modern web applications.
//! Built with configurable extensions, smart includes, and professional-grade directives.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use luminess::{Luminess, Context};
//!
//! let mut engine = Luminess::new()
//!     .with_extension("lmtp")
//!     .with_template_path("templates");
//!
//! engine.add_template("welcome", r#"
//!     <h1>{{ title | UCASE, ESCAPE_HTML, "Welcome" }}</h1>
//!     @include_header("main")
//!     <p>Hello, {{ user.name | ESCAPE_HTML, "Guest" }}!</p>
//!     @include_footer()
//! "#).unwrap();
//!
//! let mut context = Context::new();
//! context.insert("title", "my app");
//! context.insert_object("user", &serde_json::json!({
//!     "name": "James",
//!     "authenticated": true
//! }));
//!
//! let (html, content_type) = engine.render_html("welcome", &context).unwrap();
//! assert_eq!(content_type, "text/html; charset=utf-8");
//! ```
//!
//! # Key Features
//!
//! - **Blazing Fast**: JIT compilation for sub-millisecond rendering
//! - **Configurable Extensions**: Use `.lmtp`, `.html`, or any custom extension
//! - **Smart Includes**: `@include_header("admin")` with automatic fallbacks
//! - **Professional Directives**: `UCASE`, `ESCAPE_HTML`, `FORMAT_DATE`, `JSON_ENCODE`
//! - **Multiple Output Formats**: HTML, JSON, Text with proper content types
//! - **Template Path Management**: Multiple search paths with fallbacks
//! - **Zero Dependencies**: Pure Rust performance, no bloat
//!
//! # Architecture
//!
//! Luminess follows SOLID principles with clean separation:
//!
//! - **Lexer**: Tokenizes `{{ }}` and `@include` syntax
//! - **Parser**: Builds AST from tokens 
//! - **Resolver**: Finds templates with smart fallbacks
//! - **Renderer**: Executes templates with directive pipeline
//! - **Context**: Type-safe data management
//!
//! # Advanced Usage
//!
//! ```rust
//! use luminess::{Luminess, Context, LuminessConfig};
//!
//! // Advanced configuration
//! let config = LuminessConfig {
//!     extension: "lmtp".to_string(),
//!     template_paths: vec![
//!         "themes/admin".to_string(),
//!         "themes/frontend".to_string(),
//!         "themes/default".to_string(),
//!     ],
//!     enable_whitespace_control: true,
//!     enable_jit_compilation: true,
//!     cache_compiled_templates: true,
//! };
//!
//! let mut engine = Luminess::with_config(config).unwrap();
//!
//! // Use JSON data
//! let data = serde_json::json!({
//!     "post": {
//!         "title": "Hello World",
//!         "created_at": 1609459200, // Unix timestamp
//!         "content": "<p>Rich content</p>"
//!     },
//!     "user": {
//!         "name": "Developer",
//!         "role": "admin"
//!     }
//! });
//!
//! let context = Context::from_json(data).unwrap();
//! // Note: This requires blog_post.lmtp to exist in template paths
//! // let (result, _) = engine.render_html("blog_post", &context).unwrap();
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![deny(unsafe_code)]

pub mod error;
pub mod lexer;
pub mod parser;
pub mod resolver;
pub mod context;
pub mod renderer;
pub mod directives;

// Re-export main types for convenience
pub use error::{LuminessError, Result};
pub use context::Context;
pub use renderer::OutputFormat;

use std::collections::HashMap;

/// Configuration for the Luminess template engine
#[derive(Debug, Clone)]
pub struct LuminessConfig {
    /// File extension to use for templates (e.g., "lmtp", "html")
    pub extension: String,
    /// Paths to search for templates, checked in order
    pub template_paths: Vec<String>,
    /// Whether to enable whitespace control syntax ({%- -%})
    pub enable_whitespace_control: bool,
    /// Whether to enable JIT compilation for performance
    pub enable_jit_compilation: bool,
    /// Whether to cache compiled templates in memory
    pub cache_compiled_templates: bool,
}

impl Default for LuminessConfig {
    fn default() -> Self {
        Self {
            extension: "lmtp".to_string(),
            template_paths: vec!["templates".to_string()],
            enable_whitespace_control: true,
            enable_jit_compilation: true,
            cache_compiled_templates: true,
        }
    }
}

/// Main template engine interface
pub struct Luminess {
    config: LuminessConfig,
    templates: HashMap<String, parser::Template>,
    resolver: resolver::TemplateResolver,
}

impl Luminess {
    /// Create a new Luminess engine with default configuration
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use luminess::Luminess;
    /// 
    /// let engine = Luminess::new();
    /// ```
    pub fn new() -> Self {
        Self::with_config(LuminessConfig::default()).unwrap()
    }

    /// Create a new Luminess engine with custom configuration
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use luminess::{Luminess, LuminessConfig};
    /// 
    /// let config = LuminessConfig {
    ///     extension: "html".to_string(),
    ///     template_paths: vec!["views".to_string()],
    ///     ..Default::default()
    /// };
    /// 
    /// let engine = Luminess::with_config(config).unwrap();
    /// ```
    pub fn with_config(config: LuminessConfig) -> Result<Self> {
        let resolver = resolver::TemplateResolver::new(&config)?;
        
        Ok(Self {
            config,
            templates: HashMap::new(),
            resolver,
        })
    }

    /// Set the template file extension
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use luminess::Luminess;
    /// 
    /// let engine = Luminess::new().with_extension("html");
    /// ```
    pub fn with_extension(mut self, extension: &str) -> Self {
        self.config.extension = extension.to_string();
        // Recreate resolver with new config
        self.resolver = resolver::TemplateResolver::new(&self.config)
            .expect("Failed to create resolver with new extension");
        self
    }

    /// Add a template search path
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use luminess::Luminess;
    /// 
    /// let engine = Luminess::new()
    ///     .with_template_path("themes/admin")
    ///     .with_template_path("themes/default");
    /// ```
    pub fn with_template_path(mut self, path: &str) -> Self {
        self.config.template_paths.push(path.to_string());
        // Recreate resolver with new paths
        self.resolver = resolver::TemplateResolver::new(&self.config)
            .expect("Failed to create resolver with new path");
        self
    }

    /// Add a template from a string
    /// 
    /// This compiles the template immediately and stores it for later rendering.
    /// Template names should not include the file extension.
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use luminess::Luminess;
    /// 
    /// let mut engine = Luminess::new();
    /// engine.add_template("welcome", r#"
    ///     <h1>{{ title | UCASE, "Welcome" }}</h1>
    ///     <p>Hello, {{ name | ESCAPE_HTML }}!</p>
    /// "#).unwrap();
    /// ```
    pub fn add_template(&mut self, name: &str, source: &str) -> Result<()> {
        let template = self.compile_template(source)?;
        self.templates.insert(name.to_string(), template);
        Ok(())
    }

    /// Load a template from the file system
    /// 
    /// Uses the configured template paths and extension to find the template file.
    /// For example, with extension "lmtp" and template paths ["themes/admin", "themes/default"],
    /// loading "header" will look for:
    /// - themes/admin/header.lmtp
    /// - themes/default/header.lmtp
    /// 
    /// # Example
    /// 
    /// ```rust,no_run
    /// use luminess::Luminess;
    /// 
    /// let mut engine = Luminess::new();
    /// // Note: This requires header.lmtp to exist in templates/ directory
    /// engine.load_template("header").unwrap();
    /// ```
    pub fn load_template(&mut self, name: &str) -> Result<()> {
        let template_path = self.resolver.resolve_template(name)?;
        let source = std::fs::read_to_string(&template_path)
            .map_err(|e| LuminessError::io(template_path.display().to_string(), e))?;
        
        let template = self.compile_template(&source)?;
        self.templates.insert(name.to_string(), template);
        Ok(())
    }

    /// Render a template as HTML
    /// 
    /// Returns the rendered content and the appropriate Content-Type header.
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use luminess::{Luminess, Context};
    /// 
    /// let mut engine = Luminess::new();
    /// engine.add_template("page", "<h1>{{ title }}</h1>").unwrap();
    /// 
    /// let mut context = Context::new();
    /// context.insert("title", "Hello World");
    /// 
    /// let (html, content_type) = engine.render_html("page", &context).unwrap();
    /// assert_eq!(content_type, "text/html; charset=utf-8");
    /// ```
    pub fn render_html(&self, template_name: &str, context: &Context) -> Result<(String, &'static str)> {
        let content = self.render(template_name, context)?;
        Ok((content, OutputFormat::Html.content_type()))
    }

    /// Render a template as JSON
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use luminess::{Luminess, Context};
    /// 
    /// let mut engine = Luminess::new();
    /// engine.add_template("api", r#"{"message": "{{ msg | JSON_ENCODE }}"}"#).unwrap();
    /// 
    /// let mut context = Context::new();
    /// context.insert("msg", "Hello \"World\"");
    /// 
    /// let (json, content_type) = engine.render_json("api", &context).unwrap();
    /// assert_eq!(content_type, "application/json; charset=utf-8");
    /// ```
    pub fn render_json(&self, template_name: &str, context: &Context) -> Result<(String, &'static str)> {
        let content = self.render(template_name, context)?;
        Ok((content, OutputFormat::Json.content_type()))
    }

    /// Render a template as plain text
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use luminess::{Luminess, Context};
    /// 
    /// let mut engine = Luminess::new();
    /// engine.add_template("email", "Hello {{ name }}!").unwrap();
    /// 
    /// let mut context = Context::new();
    /// context.insert("name", "User");
    /// 
    /// let (text, content_type) = engine.render_text("email", &context).unwrap();
    /// assert_eq!(content_type, "text/plain; charset=utf-8");
    /// ```
    pub fn render_text(&self, template_name: &str, context: &Context) -> Result<(String, &'static str)> {
        let content = self.render(template_name, context)?;
        Ok((content, OutputFormat::Text.content_type()))
    }

    /// Core rendering function
    /// 
    /// This is the main rendering engine that processes templates with the given context.
    fn render(&self, template_name: &str, context: &Context) -> Result<String> {
        let template = self.templates.get(template_name)
            .ok_or_else(|| LuminessError::template_not_found(template_name))?;
        
        let renderer = renderer::TemplateRenderer::new(&self.config, &self.resolver);
        renderer.render(template, context)
    }

    /// Compile a template string into an internal representation
    fn compile_template(&self, source: &str) -> Result<parser::Template> {
        let mut lexer = lexer::Lexer::new(source);
        let tokens = lexer.tokenize()?;
        
        let mut parser = parser::TemplateParser::new();
        parser.parse(tokens)
    }

    /// Get engine configuration
    pub fn config(&self) -> &LuminessConfig {
        &self.config
    }

    /// List all loaded templates
    pub fn template_names(&self) -> Vec<&String> {
        self.templates.keys().collect()
    }

    /// Check if a template is loaded
    pub fn has_template(&self, name: &str) -> bool {
        self.templates.contains_key(name)
    }

    /// Clear all loaded templates
    /// 
    /// Useful for hot-reloading during development.
    pub fn clear_templates(&mut self) {
        self.templates.clear();
    }

    /// Validate template syntax without compiling
    /// 
    /// This is faster than full compilation when you only need syntax checking.
    /// 
    /// # Example
    /// 
    /// ```rust
    /// use luminess::Luminess;
    /// 
    /// let engine = Luminess::new();
    /// 
    /// assert!(engine.validate_syntax("<h1>{{ title }}</h1>").is_ok());
    /// assert!(engine.validate_syntax("<h1>{{ unclosed").is_err());
    /// ```
    pub fn validate_syntax(&self, source: &str) -> Result<()> {
        let mut lexer = lexer::Lexer::new(source);
        let tokens = lexer.tokenize()?;
        
        let mut parser = parser::TemplateParser::new();
        parser.validate_only(tokens)?;
        
        Ok(())
    }
}

impl Default for Luminess {
    fn default() -> Self {
        Self::new()
    }
}
