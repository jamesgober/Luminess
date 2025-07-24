//! # Error Handling
//! 
//! Comprehensive error system for Luminess template operations.
//! Designed for clarity, debuggability, and user-friendly messages following
//! the proven patterns established in NOML.

use std::io;
use thiserror::Error;

/// The main result type used throughout Luminess operations.
pub type Result<T> = std::result::Result<T, LuminessError>;

/// Comprehensive error types for all Luminess operations.
/// 
/// This error system provides maximum clarity about what went wrong,
/// where it happened, and how to fix it. Each variant includes enough context
/// for both developers and end users to understand and resolve issues.
#[derive(Error, Debug)]
pub enum LuminessError {
    /// Lexical analysis errors - when input cannot be tokenized
    #[error("Lexer error at line {line}, column {column}: {message}")]
    Lexer {
        /// Human-readable error message
        message: String,
        /// Line number where error occurred (1-indexed)
        line: usize,
        /// Column number where error occurred (1-indexed)
        column: usize,
        /// Optional source code snippet showing the error
        snippet: Option<String>,
    },

    /// Parsing errors - when tokens cannot be parsed into valid AST
    #[error("Parse error at line {line}, column {column}: {message}")]
    Parse {
        /// Human-readable error message
        message: String,
        /// Line number where error occurred (1-indexed)
        line: usize,
        /// Column number where error occurred (1-indexed) 
        column: usize,
        /// Optional source code snippet showing the error
        snippet: Option<String>,
    },

    /// Template not found errors
    #[error("Template '{name}' not found")]
    TemplateNotFound {
        /// The template name that was requested
        name: String,
        /// Paths that were searched
        searched_paths: Vec<String>,
    },

    /// Template resolution errors - when includes/extends cannot be resolved
    #[error("Template resolution error: {message}")]
    Resolution {
        /// Description of the resolution failure
        message: String,
        /// Template that failed to resolve
        template: String,
        /// Path being resolved
        path: Option<String>,
    },

    /// Context errors - when accessing data in template context
    #[error("Context error: {message}")]
    Context {
        /// Description of the context error
        message: String,
        /// Path that was being accessed
        path: Option<String>,
        /// Available keys at that level (for suggestions)
        available: Vec<String>,
    },

    /// Directive errors - when template directives fail
    #[error("Directive error: {directive} failed: {message}")]
    Directive {
        /// Name of the directive that failed
        directive: String,
        /// Description of the failure
        message: String,
        /// Input value that caused the failure
        input_value: Option<String>,
        /// Template location where this occurred
        location: Option<String>,
    },

    /// Type conversion errors - when values cannot be converted to requested type
    #[error("Type error: cannot convert '{value}' to {expected_type}")]
    Type {
        /// The value that couldn't be converted
        value: String,
        /// The expected type
        expected_type: String,
        /// The actual type found
        actual_type: String,
    },

    /// File I/O errors - wraps std::io::Error with additional context
    #[error("File error for '{path}': {source}")]
    Io {
        /// Path to the file that caused the error
        path: String,
        /// The underlying I/O error
        #[source]
        source: io::Error,
    },

    /// Configuration errors
    #[error("Configuration error: {message}")]
    Config {
        /// Description of the configuration problem
        message: String,
        /// The configuration key that caused the issue
        key: Option<String>,
    },

    /// Rendering errors - general template rendering failures
    #[error("Rendering error in template '{template}': {message}")]
    Render {
        /// Template that failed to render
        template: String,
        /// Description of the rendering failure
        message: String,
        /// Line in template where error occurred
        line: Option<usize>,
    },

    /// Circular reference errors (for includes and extends)
    #[error("Circular reference detected: {chain}")]
    CircularReference {
        /// The chain of references that caused the cycle
        chain: String,
    },

    /// Internal errors - these should never happen in normal operation
    #[error("Internal error: {message}")]
    Internal {
        /// Description of the internal error
        message: String,
        /// Optional context about where this occurred
        context: Option<String>,
    },
}

impl LuminessError {
    /// Create a lexer error with position information
    pub fn lexer(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self::Lexer {
            message: message.into(),
            line,
            column,
            snippet: None,
        }
    }

    /// Create a lexer error with source code snippet
    pub fn lexer_with_snippet(
        message: impl Into<String>,
        line: usize,
        column: usize,
        snippet: impl Into<String>,
    ) -> Self {
        Self::Lexer {
            message: message.into(),
            line,
            column,
            snippet: Some(snippet.into()),
        }
    }

    /// Create a parse error with position information
    pub fn parse(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self::Parse {
            message: message.into(),
            line,
            column,
            snippet: None,
        }
    }

    /// Create a parse error with source code snippet
    pub fn parse_with_snippet(
        message: impl Into<String>,
        line: usize,
        column: usize,
        snippet: impl Into<String>,
    ) -> Self {
        Self::Parse {
            message: message.into(),
            line,
            column,
            snippet: Some(snippet.into()),
        }
    }

    /// Create a template not found error
    pub fn template_not_found(name: impl Into<String>) -> Self {
        Self::TemplateNotFound {
            name: name.into(),
            searched_paths: Vec::new(),
        }
    }

    /// Create a template not found error with searched paths
    pub fn template_not_found_with_paths(
        name: impl Into<String>,
        searched_paths: Vec<String>,
    ) -> Self {
        Self::TemplateNotFound {
            name: name.into(),
            searched_paths,
        }
    }

    /// Create a template resolution error
    pub fn resolution(
        message: impl Into<String>,
        template: impl Into<String>,
    ) -> Self {
        Self::Resolution {
            message: message.into(),
            template: template.into(),
            path: None,
        }
    }

    /// Create a context error
    pub fn context(message: impl Into<String>) -> Self {
        Self::Context {
            message: message.into(),
            path: None,
            available: Vec::new(),
        }
    }

    /// Create a context error with path and suggestions
    pub fn context_with_suggestions(
        message: impl Into<String>,
        path: impl Into<String>,
        available: Vec<String>,
    ) -> Self {
        Self::Context {
            message: message.into(),
            path: Some(path.into()),
            available,
        }
    }

    /// Create a directive error
    pub fn directive(
        directive: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::Directive {
            directive: directive.into(),
            message: message.into(),
            input_value: None,
            location: None,
        }
    }

    /// Create a directive error with input value
    pub fn directive_with_input(
        directive: impl Into<String>,
        message: impl Into<String>,
        input_value: impl Into<String>,
    ) -> Self {
        Self::Directive {
            directive: directive.into(),
            message: message.into(),
            input_value: Some(input_value.into()),
            location: None,
        }
    }

    /// Create a type conversion error
    pub fn type_error(
        value: impl Into<String>,
        expected: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        Self::Type {
            value: value.into(),
            expected_type: expected.into(),
            actual_type: actual.into(),
        }
    }

    /// Create an I/O error with path context
    pub fn io(path: impl Into<String>, error: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source: error,
        }
    }

    /// Create a configuration error
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config {
            message: message.into(),
            key: None,
        }
    }

    /// Create a configuration error with key context
    pub fn config_key(message: impl Into<String>, key: impl Into<String>) -> Self {
        Self::Config {
            message: message.into(),
            key: Some(key.into()),
        }
    }

    /// Create a rendering error
    pub fn render(template: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Render {
            template: template.into(),
            message: message.into(),
            line: None,
        }
    }

    /// Create a rendering error with line information
    pub fn render_at_line(
        template: impl Into<String>,
        message: impl Into<String>,
        line: usize,
    ) -> Self {
        Self::Render {
            template: template.into(),
            message: message.into(),
            line: Some(line),
        }
    }

    /// Create a circular reference error
    pub fn circular_reference(chain: impl Into<String>) -> Self {
        Self::CircularReference {
            chain: chain.into(),
        }
    }

    /// Create an internal error (should be used sparingly)
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
            context: None,
        }
    }

    /// Create an internal error with context
    pub fn internal_with_context(
        message: impl Into<String>,
        context: impl Into<String>,
    ) -> Self {
        Self::Internal {
            message: message.into(),
            context: Some(context.into()),
        }
    }

    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            // Lexer and parse errors are generally not recoverable
            LuminessError::Lexer { .. } => false,
            LuminessError::Parse { .. } => false,
            // Template not found might be recoverable with fallbacks
            LuminessError::TemplateNotFound { .. } => true,
            // Resolution errors might be recoverable
            LuminessError::Resolution { .. } => true,
            // Context errors are recoverable (use defaults, etc.)
            LuminessError::Context { .. } => true,
            // Directive errors might be recoverable
            LuminessError::Directive { .. } => true,
            // Type errors might be recoverable with conversion
            LuminessError::Type { .. } => true,
            // I/O errors depend on the specific error
            LuminessError::Io { source, .. } => match source.kind() {
                io::ErrorKind::NotFound => true,
                io::ErrorKind::PermissionDenied => false,
                _ => true,
            },
            // Config errors are usually recoverable
            LuminessError::Config { .. } => true,
            // Render errors might be recoverable
            LuminessError::Render { .. } => true,
            // Circular references are not recoverable
            LuminessError::CircularReference { .. } => false,
            // Internal errors are not recoverable
            LuminessError::Internal { .. } => false,
        }
    }

    /// Get the error category for metrics/logging
    pub fn category(&self) -> &'static str {
        match self {
            LuminessError::Lexer { .. } => "lexer",
            LuminessError::Parse { .. } => "parse",
            LuminessError::TemplateNotFound { .. } => "template_not_found",
            LuminessError::Resolution { .. } => "resolution",
            LuminessError::Context { .. } => "context",
            LuminessError::Directive { .. } => "directive",
            LuminessError::Type { .. } => "type_conversion",
            LuminessError::Io { .. } => "io",
            LuminessError::Config { .. } => "config",
            LuminessError::Render { .. } => "render",
            LuminessError::CircularReference { .. } => "circular_reference",
            LuminessError::Internal { .. } => "internal",
        }
    }

    /// Get a user-friendly error message with suggestions
    pub fn user_message(&self) -> String {
        match self {
            LuminessError::Lexer { message, line, column, snippet } => {
                let mut msg = format!("Template syntax error on line {}, column {}: {}", line, column, message);
                if let Some(snippet) = snippet {
                    msg.push_str(&format!("\n\n{}", snippet));
                }
                msg.push_str("\n\nTip: Check for missing quotes, unclosed brackets, or invalid syntax.");
                msg
            }
            LuminessError::Parse { message, line, column, snippet } => {
                let mut msg = format!("Template parse error on line {}, column {}: {}", line, column, message);
                if let Some(snippet) = snippet {
                    msg.push_str(&format!("\n\n{}", snippet));
                }
                msg.push_str("\n\nTip: Verify template structure and directive syntax.");
                msg
            }
            LuminessError::TemplateNotFound { name, searched_paths } => {
                let mut msg = format!("Template '{}' could not be found.", name);
                if !searched_paths.is_empty() {
                    msg.push_str("\n\nSearched in:");
                    for path in searched_paths {
                        msg.push_str(&format!("\n  - {}", path));
                    }
                }
                msg.push_str("\n\nTip: Check template name spelling and ensure files exist in template paths.");
                msg
            }
            LuminessError::Context { message, path, available } => {
                let mut msg = message.clone();
                if let Some(path) = path {
                    msg = format!("Context error at '{}': {}", path, msg);
                }
                if !available.is_empty() {
                    msg.push_str("\n\nAvailable keys:");
                    for key in available {
                        msg.push_str(&format!("\n  - {}", key));
                    }
                }
                msg
            }
            LuminessError::Directive { directive, message, input_value, .. } => {
                let mut msg = format!("Directive '{}' failed: {}", directive, message);
                if let Some(input) = input_value {
                    msg.push_str(&format!("\n\nInput value: '{}'", input));
                }
                msg.push_str("\n\nTip: Check directive syntax and input value type.");
                msg
            }
            _ => self.to_string(),
        }
    }
}

/// Convert from std::io::Error to LuminessError
impl From<io::Error> for LuminessError {
    fn from(error: io::Error) -> Self {
        Self::Io {
            path: "<unknown>".to_string(),
            source: error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_creation_and_display() {
        let err = LuminessError::lexer("Invalid token", 10, 5);
        assert_eq!(err.to_string(), "Lexer error at line 10, column 5: Invalid token");
    }

    #[test]
    fn error_categories() {
        let lexer_err = LuminessError::lexer("test", 1, 1);
        assert_eq!(lexer_err.category(), "lexer");
        assert!(!lexer_err.is_recoverable());

        let context_err = LuminessError::context("test");
        assert_eq!(context_err.category(), "context");
        assert!(context_err.is_recoverable());
    }

    #[test]
    fn user_friendly_messages() {
        let err = LuminessError::context_with_suggestions(
            "Key not found",
            "user.unknown",
            vec!["user.name".to_string(), "user.email".to_string()],
        );
        let msg = err.user_message();
        assert!(msg.contains("user.unknown"));
        assert!(msg.contains("user.name"));
        assert!(msg.contains("user.email"));
    }

    #[test]
    fn template_not_found_with_paths() {
        let err = LuminessError::template_not_found_with_paths(
            "missing_template",
            vec!["themes/admin".to_string(), "themes/default".to_string()],
        );
        let msg = err.user_message();
        assert!(msg.contains("missing_template"));
        assert!(msg.contains("themes/admin"));
        assert!(msg.contains("themes/default"));
    }

    #[test]
    fn directive_errors_with_input() {
        let err = LuminessError::directive_with_input(
            "UCASE",
            "Cannot convert to uppercase",
            "null value",
        );
        assert_eq!(err.category(), "directive");
        assert!(err.is_recoverable());
        
        let msg = err.user_message();
        assert!(msg.contains("UCASE"));
        assert!(msg.contains("null value"));
    }

    #[test]
    fn io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let luminess_err = LuminessError::from(io_err);
        
        match luminess_err {
            LuminessError::Io { path, .. } => {
                assert_eq!(path, "<unknown>");
            }
            _ => panic!("Expected Io error"),
        }
    }

    #[test]
    fn error_recoverability() {
        // Non-recoverable errors
        assert!(!LuminessError::lexer("test", 1, 1).is_recoverable());
        assert!(!LuminessError::parse("test", 1, 1).is_recoverable());
        assert!(!LuminessError::circular_reference("a->b->a").is_recoverable());
        assert!(!LuminessError::internal("test").is_recoverable());
        
        // Recoverable errors
        assert!(LuminessError::template_not_found("test").is_recoverable());
        assert!(LuminessError::context("test").is_recoverable());
        assert!(LuminessError::directive("TEST", "failed").is_recoverable());
        assert!(LuminessError::config("test").is_recoverable());
    }
}
