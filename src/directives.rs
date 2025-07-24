//! # Template Directives
//! 
//! Professional-grade directives for data transformation.

use crate::error::{LuminessError, Result};

/// Apply a directive to a string value
pub fn apply_directive(input: &str, directive: &str) -> Result<String> {
    match directive {
        "TRIM" => Ok(input.trim().to_string()),
        "UCASE" => Ok(input.to_uppercase()),
        "LCASE" => Ok(input.to_lowercase()),
        "ESCAPE_HTML" => Ok(html_escape(input)),
        "STRIP_TAGS" => Ok(strip_html_tags(input)),
        "JSON_ENCODE" => {
            match serde_json::to_string(input) {
                Ok(json) => Ok(json),
                Err(_) => Err(LuminessError::directive("JSON_ENCODE", "Failed to encode as JSON")),
            }
        }
        _ => Err(LuminessError::directive(directive, "Unknown directive")),
    }
}

/// HTML escape for security
pub fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Strip HTML tags (basic implementation)
pub fn strip_html_tags(input: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    
    result
}

/// Format date (placeholder - would need chrono for full implementation)
pub fn format_date(timestamp: i64, format: &str) -> Result<String> {
    // Simplified implementation - in production would use chrono
    Ok(format!("formatted_date_{}_{}", timestamp, format))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_directives() {
        assert_eq!(apply_directive("  hello  ", "TRIM").unwrap(), "hello");
        assert_eq!(apply_directive("hello", "UCASE").unwrap(), "HELLO");
        assert_eq!(apply_directive("HELLO", "LCASE").unwrap(), "hello");
    }
    
    #[test]
    fn html_escaping() {
        let input = "<script>alert('xss')</script>";
        let expected = "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;";
        assert_eq!(apply_directive(input, "ESCAPE_HTML").unwrap(), expected);
    }
    
    #[test]
    fn tag_stripping() {
        let input = "<p>Hello <strong>world</strong>!</p>";
        let expected = "Hello world!";
        assert_eq!(apply_directive(input, "STRIP_TAGS").unwrap(), expected);
    }
}
