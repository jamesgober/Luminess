//! # Lexical Analysis
//! 
//! High-performance lexer for Luminess template syntax.

use crate::error::{LuminessError, Result};

/// Position information for error reporting and debugging
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Line number in the source (starting at 1)
    pub line: usize,
    /// Column number in the source (starting at 1)
    pub column: usize,
    /// Byte offset from the start of the source
    pub offset: usize,
}

impl Position {
    /// Creates a new `Position` at the start of the source (line 1, column 1, offset 0).
        pub fn new() -> Self {
            Self { line: 1, column: 1, offset: 0 }
        }
    
        /// Advances the position by one character, updating line and column numbers.
        ///
        /// If the character is a newline (`\n`), the line number is incremented and the column is reset to 1.
        /// Otherwise, the column number is incremented.
        pub fn advance(&mut self, ch: u8) {
            self.offset += 1;
            if ch == b'\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
}

// spell-checker:ignore Luminess
/// Token types that represent all Luminess template constructs
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    /// Plain text content.
    Text(String),
    /// Variable interpolation.
    ///
    /// # Fields
    /// - `path`: Path to the variable, split by dots.
    /// - `directives`: List of directives to apply.
    /// - `default`: Optional default value if variable is missing.
    Variable {
        /// Path to the variable, split by dots.
        path: Vec<String>,
        /// List of directives to apply.
        directives: Vec<String>,
        /// Optional default value if variable is missing.
        default: Option<String>,
    },
    /// Include another template.
    ///
    /// # Fields
    /// - `template`: Name of the template to include.
    Include {
        /// Name of the template to include.
        template: String,
    },
    /// Include a header template variant.
    ///
    /// # Fields
    /// - `variant`: Optional variant name.
    IncludeHeader {
        /// Optional variant name.
        variant: Option<String>,
    },
    /// Include a footer template variant.
    ///
    /// # Fields
    /// - `variant`: Optional variant name.
    IncludeFooter {
        /// Optional variant name.
        variant: Option<String>,
    },
    /// If block directive.
    ///
    /// # Fields
    /// - `condition`: Condition expression.
    If {
        /// Condition expression.
        condition: String,
    },
    /// Else block directive.
    Else,
    /// EndIf block directive.
    EndIf,
    /// For loop directive.
    ///
    /// # Fields
    /// - `item`: Loop variable name.
    /// - `collection`: Collection to iterate over.
    For {
        /// Loop variable name.
        item: String,
        /// Collection to iterate over.
        collection: String,
    },
    /// EndFor block directive.
    EndFor,
    /// Start of a raw block.
    RawStart,
    /// End of a raw block.
    RawEnd,
    /// Trim whitespace to the left.
    TrimLeft,
    /// Trim whitespace to the right.
    TrimRight,
}

/// High-performance lexer
pub struct Lexer<'a> {
    source: &'a [u8],
    position: usize,
    pos: Position,
    in_raw_block: bool,
}

impl<'a> Lexer<'a> {
    /// Creates a new lexer for the given source string.
    pub fn new(source: &'a str) -> Self {
        Self {
            source: source.as_bytes(),
            position: 0,
            pos: Position::new(),
            in_raw_block: false,
        }
    }

    /// Tokenizes the source string into a vector of `TokenType`.
    pub fn tokenize(&mut self) -> Result<Vec<TokenType>> {
        let mut tokens = Vec::new();
        let mut current_text = String::new();

        while self.position < self.source.len() {
            if self.in_raw_block {
                // spell-checker:ignore endraw
                if self.peek_sequence(b"{% endraw %}") {
                    if !current_text.is_empty() {
                        tokens.push(TokenType::Text(current_text.clone()));
                        current_text.clear();
                    }
                    tokens.push(TokenType::RawEnd);
                    self.advance_by(12);
                    self.in_raw_block = false;
                } else {
                    current_text.push(self.advance_char());
                }
            } else if self.peek_sequence(b"{{") {
                if !current_text.is_empty() {
                    tokens.push(TokenType::Text(current_text.clone()));
                    current_text.clear();
                }
                tokens.push(self.parse_variable()?);
            } else if self.peek_sequence(b"@include") {
                if !current_text.is_empty() {
                    tokens.push(TokenType::Text(current_text.clone()));
                    current_text.clear();
                }
                tokens.push(self.parse_include()?);
            } else if self.peek_sequence(b"{%") {
                if !current_text.is_empty() {
                    tokens.push(TokenType::Text(current_text.clone()));
                    current_text.clear();
                }
                tokens.push(self.parse_block_directive()?);
            } else {
                current_text.push(self.advance_char());
            }
        }

        if !current_text.is_empty() {
            tokens.push(TokenType::Text(current_text));
        }

        Ok(tokens)
    }

    fn parse_variable(&mut self) -> Result<TokenType> {
        let start_pos = self.pos;
        self.advance_by(2);
        
        let content = self.read_until(b"}}")
            .ok_or_else(|| LuminessError::lexer("Unclosed variable", start_pos.line, start_pos.column))?;
        
        self.advance_by(2);

        let content = content.trim();
        let parts: Vec<&str> = content.splitn(2, '|').collect();
        let var_path = parts[0].trim();
        
        let path: Vec<String> = var_path
            .split('.')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if path.is_empty() {
            return Err(LuminessError::lexer("Empty variable path", start_pos.line, start_pos.column));
        }

        let (directives, default) = if parts.len() > 1 {
            self.parse_directive_chain(parts[1].trim())?
        } else {
            (Vec::new(), None)
        };

        Ok(TokenType::Variable { path, directives, default })
    }

    fn parse_directive_chain(&self, input: &str) -> Result<(Vec<String>, Option<String>)> {
        let mut directives = Vec::new();
        let mut default = None;
        
        let parts: Vec<&str> = input.split(',').map(|s| s.trim()).collect();
        
        for part in parts {
            if part.starts_with('"') && part.ends_with('"') {
                default = Some(part[1..part.len()-1].to_string());
            } else if part.starts_with('\'') && part.ends_with('\'') {
                default = Some(part[1..part.len()-1].to_string());
            } else if part.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
                directives.push(part.to_string());
            } else {
                return Err(LuminessError::lexer(
                    format!("Invalid directive '{}' - directives must be UPPERCASE", part),
                    self.pos.line, self.pos.column,
                ));
            }
        }
        
        Ok((directives, default))
    }

    fn parse_include(&mut self) -> Result<TokenType> {
        let start_pos = self.pos;
        
        if self.peek_sequence(b"@include_header") {
            self.advance_by(15);
            self.skip_whitespace();
            
            if self.peek_sequence(b"(") {
                self.advance_by(1);
                let variant = self.parse_include_parameter()?;
                self.expect_char(b')')?;
                Ok(TokenType::IncludeHeader { variant })
            } else {
                Ok(TokenType::IncludeHeader { variant: None })
            }
        } else if self.peek_sequence(b"@include_footer") {
            self.advance_by(15);
            self.skip_whitespace();
            
            if self.peek_sequence(b"(") {
                self.advance_by(1);
                let variant = self.parse_include_parameter()?;
                self.expect_char(b')')?;
                Ok(TokenType::IncludeFooter { variant })
            } else {
                Ok(TokenType::IncludeFooter { variant: None })
            }
        } else if self.peek_sequence(b"@include") {
            self.advance_by(8);
            self.skip_whitespace();
            self.expect_char(b'(')?;
            
            let template = self.parse_include_parameter()?
                .ok_or_else(|| LuminessError::lexer("@include requires a template name", start_pos.line, start_pos.column))?;
            
            self.expect_char(b')')?;
            Ok(TokenType::Include { template })
        } else {
            Err(LuminessError::lexer("Invalid include directive", start_pos.line, start_pos.column))
        }
    }

    fn parse_include_parameter(&mut self) -> Result<Option<String>> {
        self.skip_whitespace();
        
        if self.peek_sequence(b")") {
            return Ok(None);
        }
        
        let quote_char = if self.peek_sequence(b"\"") {
            self.advance_by(1);
            b'"'
        } else if self.peek_sequence(b"'") {
            self.advance_by(1);
            b'\''
        } else {
            return Err(LuminessError::lexer("Include parameter must be quoted", self.pos.line, self.pos.column));
        };
        
        let mut param = String::new();
        while self.position < self.source.len() {
            let ch = self.current_byte();
            if ch == quote_char {
                self.advance_by(1);
                self.skip_whitespace();
                return Ok(Some(param));
            }
            param.push(self.advance_char());
        }
        
        Err(LuminessError::lexer("Unterminated string in include parameter", self.pos.line, self.pos.column))
    }

    fn parse_block_directive(&mut self) -> Result<TokenType> {
        let start_pos = self.pos;
        self.advance_by(2);
        
        let content = self.read_until(b"%}")
            .ok_or_else(|| LuminessError::lexer("Unclosed block directive", start_pos.line, start_pos.column))?;
        
        self.advance_by(2);
        
        let content = content.trim();
        let parts: Vec<&str> = content.split_whitespace().collect();
        
        if parts.is_empty() {
            return Err(LuminessError::lexer("Empty block directive", start_pos.line, start_pos.column));
        }
        
        match parts[0] {
            "if" => {
                if parts.len() < 2 {
                    return Err(LuminessError::lexer("if directive requires a condition", start_pos.line, start_pos.column));
                }
                Ok(TokenType::If { condition: parts[1..].join(" ") })
            }
            "else" => Ok(TokenType::Else),
            "endif" => Ok(TokenType::EndIf),
            "for" => {
                if parts.len() < 4 || parts[2] != "in" {
                    return Err(LuminessError::lexer("for directive must be 'for item in collection'", start_pos.line, start_pos.column));
                }
                Ok(TokenType::For { item: parts[1].to_string(), collection: parts[3].to_string() })
            }
            "endfor" => Ok(TokenType::EndFor),
            "raw" => {
                self.in_raw_block = true;
                Ok(TokenType::RawStart)
            }
            "endraw" => {
                self.in_raw_block = false;
                Ok(TokenType::RawEnd)
            }
            _ => Err(LuminessError::lexer(format!("Unknown block directive: {}", parts[0]), start_pos.line, start_pos.column))
        }
    }

    #[inline(always)]
    fn peek_sequence(&self, sequence: &[u8]) -> bool {
        if self.position + sequence.len() > self.source.len() {
            return false;
        }
        &self.source[self.position..self.position + sequence.len()] == sequence
    }

    #[inline(always)]
    fn current_byte(&self) -> u8 {
        if self.position < self.source.len() {
            self.source[self.position]
        } else {
            0
        }
    }

    fn advance_char(&mut self) -> char {
        let byte = self.source[self.position];
        self.pos.advance(byte);
        self.position += 1;
        byte as char
    }

    fn advance_by(&mut self, count: usize) {
        for _ in 0..count {
            if self.position < self.source.len() {
                let byte = self.source[self.position];
                self.pos.advance(byte);
                self.position += 1;
            }
        }
    }

    fn read_until(&mut self, delimiter: &[u8]) -> Option<String> {
        let start = self.position;
        while self.position < self.source.len() {
            if self.peek_sequence(delimiter) {
                let content = &self.source[start..self.position];
                return Some(String::from_utf8_lossy(content).to_string());
            }
            self.advance_char();
        }
        None
    }

    fn skip_whitespace(&mut self) {
        while self.position < self.source.len() {
            let ch = self.current_byte();
            if ch.is_ascii_whitespace() {
                self.advance_char();
            } else {
                break;
            }
        }
    }

    fn expect_char(&mut self, expected: u8) -> Result<()> {
        if self.current_byte() == expected {
            self.advance_char();
            Ok(())
        } else {
            Err(LuminessError::lexer(
                format!("Expected '{}', found '{}'", expected as char, self.current_byte() as char),
                self.pos.line, self.pos.column,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_tokenization() {
        let mut lexer = Lexer::new("{{ user.name | UCASE, 'Default' }}");
        let tokens = lexer.tokenize().unwrap();
        
        assert_eq!(tokens.len(), 1);
        match &tokens[0] {
            TokenType::Variable { path, directives, default } => {
                assert_eq!(path, &vec!["user".to_string(), "name".to_string()]);
                assert_eq!(directives, &vec!["UCASE".to_string()]);
                assert_eq!(default, &Some("Default".to_string()));
            }
            _ => panic!("Expected variable token"),
        }
    }

    #[test]
    fn include_tokenization() {
        let mut lexer = Lexer::new("@include_header('admin')");
        let tokens = lexer.tokenize().unwrap();
        
        assert_eq!(tokens.len(), 1);
        match &tokens[0] {
            TokenType::IncludeHeader { variant } => {
                assert_eq!(variant, &Some("admin".to_string()));
            }
            _ => panic!("Expected include_header token"),
        }
    }
}
