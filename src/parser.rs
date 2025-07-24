//! # Template Parsing
//! 
//! Converts tokens into an Abstract Syntax Tree (AST) for execution.

use crate::lexer::TokenType;
use crate::error::Result;

/// Represents a parsed template ready for execution
#[derive(Debug, Clone)]
/// Represents a parsed template ready for execution
pub struct Template {
    /// The list of AST nodes parsed from the template
    pub nodes: Vec<AstNode>,
}

/// AST node types for template execution
#[derive(Debug, Clone)]
pub enum AstNode {
    /// Plain text node
    /// Holds the text content.
    Text(String),
    /// Variable node with path, directives, and optional default value
    Variable {
        /// Path to the variable
        path: Vec<String>,
        /// Directives applied to the variable
        directives: Vec<String>,
        /// Default value if the variable is not found
        default: Option<String>,
    },
    /// Include another template
    Include {
        /// Name of the template to include
        template: String,
    },
    /// Include a header section, optionally with a variant
    IncludeHeader {
        /// Optional variant for the header
        variant: Option<String>,
    },
    /// Include a footer section, optionally with a variant
    IncludeFooter {
        /// Optional variant for the footer
        variant: Option<String>,
    },
    /// Conditional node with condition, body, and optional else body
    Conditional {
        /// The condition to evaluate
        condition: String,
        /// AST nodes for the body if the condition is true
        body: Vec<AstNode>,
        /// AST nodes for the else body if the condition is false
        else_body: Option<Vec<AstNode>>,
    },
    /// Loop node for iterating over a collection
    Loop {
        /// Name of the item variable in the loop
        item: String,
        /// Name of the collection to iterate over
        collection: String,
        /// AST nodes for the loop body
        body: Vec<AstNode>,
    },
    /// Raw text node
    /// Holds the raw text content.
    Raw(String),
}

/// Template parser that converts tokens to AST
pub struct TemplateParser;

impl TemplateParser {
    /// Creates a new TemplateParser.
    pub fn new() -> Self {
        Self
    }
    
    /// Parses a vector of tokens into a Template AST.
    pub fn parse(&mut self, tokens: Vec<TokenType>) -> Result<Template> {
        let nodes = self.parse_nodes(&tokens, 0)?.0;
        Ok(Template { nodes })
    }
    
    /// Validates the tokens without returning the AST.
    pub fn validate_only(&mut self, tokens: Vec<TokenType>) -> Result<()> {
        self.parse_nodes(&tokens, 0)?;
        Ok(())
    }
    
    fn parse_nodes(&self, tokens: &[TokenType], start: usize) -> Result<(Vec<AstNode>, usize)> {
        let mut nodes = Vec::new();
        let mut i = start;
        
        while i < tokens.len() {
            match &tokens[i] {
                TokenType::Text(text) => {
                    nodes.push(AstNode::Text(text.clone()));
                    i += 1;
                }
                TokenType::Variable { path, directives, default } => {
                    nodes.push(AstNode::Variable {
                        path: path.clone(),
                        directives: directives.clone(),
                        default: default.clone(),
                    });
                    i += 1;
                }
                TokenType::Include { template } => {
                    nodes.push(AstNode::Include { template: template.clone() });
                    i += 1;
                }
                TokenType::IncludeHeader { variant } => {
                    nodes.push(AstNode::IncludeHeader { variant: variant.clone() });
                    i += 1;
                }
                TokenType::IncludeFooter { variant } => {
                    nodes.push(AstNode::IncludeFooter { variant: variant.clone() });
                    i += 1;
                }
                TokenType::If { condition } => {
                    let (body, else_body, end_pos) = self.parse_conditional(tokens, i + 1)?;
                    nodes.push(AstNode::Conditional {
                        condition: condition.clone(),
                        body,
                        else_body,
                    });
                    i = end_pos;
                }
                TokenType::For { item, collection } => {
                    let (body, end_pos) = self.parse_loop(tokens, i + 1)?;
                    nodes.push(AstNode::Loop {
                        item: item.clone(),
                        collection: collection.clone(),
                        body,
                    });
                    i = end_pos;
                }
                TokenType::RawStart => {
                    let (content, end_pos) = self.parse_raw(tokens, i + 1)?;
                    nodes.push(AstNode::Raw(content));
                    i = end_pos;
                }
                _ => i += 1,
            }
        }
        
        Ok((nodes, tokens.len()))
    }
    
    fn parse_conditional(&self, tokens: &[TokenType], start: usize) -> Result<(Vec<AstNode>, Option<Vec<AstNode>>, usize)> {
        let mut i = start;
        let mut body = Vec::new();
        let mut else_body = None;
        
        while i < tokens.len() {
            match &tokens[i] {
                TokenType::Else => {
                    let (else_nodes, end_pos) = self.parse_nodes(tokens, i + 1)?;
                    else_body = Some(else_nodes);
                    i = end_pos;
                    break;
                }
                TokenType::EndIf => {
                    return Ok((body, else_body, i + 1));
                }
                _ => {
                    let (mut nodes, next_pos) = self.parse_nodes(tokens, i)?;
                    body.append(&mut nodes);
                    i = next_pos;
                }
            }
        }
        
        Ok((body, else_body, i))
    }
    
    fn parse_loop(&self, tokens: &[TokenType], start: usize) -> Result<(Vec<AstNode>, usize)> {
        let mut i = start;
        let mut body = Vec::new();
        
        while i < tokens.len() {
            match &tokens[i] {
                TokenType::EndFor => {
                    return Ok((body, i + 1));
                }
                _ => {
                    let (mut nodes, next_pos) = self.parse_nodes(tokens, i)?;
                    body.append(&mut nodes);
                    i = next_pos;
                }
            }
        }
        
        Ok((body, i))
    }
    
    fn parse_raw(&self, tokens: &[TokenType], start: usize) -> Result<(String, usize)> {
        let mut i = start;
        let mut content = String::new();
        
        while i < tokens.len() {
            match &tokens[i] {
                TokenType::RawEnd => {
                    return Ok((content, i + 1));
                }
                TokenType::Text(text) => {
                    content.push_str(text);
                    i += 1;
                }
                _ => i += 1,
            }
        }
        
        Ok((content, i))
    }
}

impl Default for TemplateParser {
    fn default() -> Self {
        Self::new()
    }
}
