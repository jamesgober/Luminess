//! # Template Renderer
//! 
//! Executes parsed templates with blazing fast performance.

use crate::error::{LuminessError, Result};
use crate::parser::{Template, AstNode};
use crate::context::Context;
use crate::resolver::TemplateResolver;
use crate::directives;
use crate::LuminessConfig;

/// Output format specifications
#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    /// HTML output format
    Html,
    /// JSON output format
    Json,
    /// Plain text output format
    Text,
    /// XML output format
    Xml,
}

impl OutputFormat {
    /// Returns the content type string for the output format.
    pub fn content_type(&self) -> &'static str {
        match self {
            OutputFormat::Html => "text/html; charset=utf-8",
            OutputFormat::Json => "application/json; charset=utf-8",
            OutputFormat::Text => "text/plain; charset=utf-8",
            OutputFormat::Xml => "application/xml; charset=utf-8",
        }
    }
}

/// High-performance template renderer
pub struct TemplateRenderer<'a> {
    /// Template resolver used for loading templates
    resolver: &'a TemplateResolver,
}

impl<'a> TemplateRenderer<'a> {
    /// Creates a new TemplateRenderer with the given resolver.
    pub fn new(_config: &'a LuminessConfig, resolver: &'a TemplateResolver) -> Self {
        Self {
            resolver,
        }
    }
    
    /// Render a template with the given context
    pub fn render(&self, template: &Template, context: &Context) -> Result<String> {
        let mut output = String::new();
        self.render_nodes(&template.nodes, context, &mut output)?;
        Ok(output)
    }
    
    /// Render a list of AST nodes
    fn render_nodes(&self, nodes: &[AstNode], context: &Context, output: &mut String) -> Result<()> {
        for node in nodes {
            self.render_node(node, context, output)?;
        }
        Ok(())
    }
    
    /// Render a single AST node
    fn render_node(&self, node: &AstNode, context: &Context, output: &mut String) -> Result<()> {
        match node {
            AstNode::Text(text) => {
                output.push_str(text);
            }
            
            AstNode::Variable { path, directives, default } => {
                let value = if let Some(value) = context.get_value(path) {
                    context.value_to_string(value)
                } else if let Some(default) = default {
                    default.clone()
                } else {
                    String::new()
                };
                
                let final_value = self.apply_directives(&value, directives)?;
                output.push_str(&final_value);
            }
            
            AstNode::Include { template } => {
                let included = self.load_and_render_include(template, context)?;
                output.push_str(&included);
            }
            
            AstNode::IncludeHeader { variant } => {
                let included = self.render_header_include(variant.as_deref(), context)?;
                output.push_str(&included);
            }
            
            AstNode::IncludeFooter { variant } => {
                let included = self.render_footer_include(variant.as_deref(), context)?;
                output.push_str(&included);
            }
            
            AstNode::Conditional { condition, body, else_body } => {
                let should_render = self.evaluate_condition(condition, context)?;
                
                if should_render {
                    self.render_nodes(body, context, output)?;
                } else if let Some(else_body) = else_body {
                    self.render_nodes(else_body, context, output)?;
                }
            }
            
            AstNode::Loop { item, collection, body } => {
                if let Some(collection_value) = context.get_value(&vec![collection.clone()]) {
                    if let Some(array) = collection_value.as_array() {
                        for item_value in array {
                            // Create new context with loop item
                            let mut loop_context = context.clone();
                            loop_context.insert_object(item, item_value);
                            
                            self.render_nodes(body, &loop_context, output)?;
                        }
                    }
                }
            }
            
            AstNode::Raw(content) => {
                output.push_str(content);
            }
        }
        
        Ok(())
    }
    
    /// Apply directive chain to a value
    fn apply_directives(&self, input: &str, directives: &[String]) -> Result<String> {
        let mut result = input.to_string();
        
        for directive in directives {
            result = directives::apply_directive(&result, directive)?;
        }
        
        Ok(result)
    }
    
    /// Evaluate a conditional expression
    fn evaluate_condition(&self, condition: &str, context: &Context) -> Result<bool> {
        // Simplified condition evaluation
        // In production, would parse complex expressions
        let path: Vec<String> = condition.split('.').map(|s| s.to_string()).collect();
        Ok(context.is_truthy(&path))
    }
    
    /// Load and render an included template
    fn load_and_render_include(&self, template_name: &str, _context: &Context) -> Result<String> {
        // In a full implementation, would cache parsed templates
        let template_path = self.resolver.resolve_template(template_name)?;
        let content = std::fs::read_to_string(&template_path)
            .map_err(|e| LuminessError::io(template_path.display().to_string(), e))?;
        
        // For now, just return the raw content
        // In production, would parse and render the included template
        Ok(content)
    }
    
    /// Render header include with variant support
    fn render_header_include(&self, variant: Option<&str>, _context: &Context) -> Result<String> {
        let template_path = self.resolver.resolve_header(variant)?;
        let content = std::fs::read_to_string(&template_path)
            .map_err(|e| LuminessError::io(template_path.display().to_string(), e))?;
        
        Ok(content)
    }
    
    /// Render footer include with variant support
    fn render_footer_include(&self, variant: Option<&str>, _context: &Context) -> Result<String> {
        let template_path = self.resolver.resolve_footer(variant)?;
        let content = std::fs::read_to_string(&template_path)
            .map_err(|e| LuminessError::io(template_path.display().to_string(), e))?;
        
        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::AstNode;

    #[test]
    fn basic_rendering() {
        let config = crate::LuminessConfig::default();
        let resolver = crate::resolver::TemplateResolver::new(&config).unwrap();
        let renderer = TemplateRenderer::new(&config, &resolver);
        
        let template = Template {
            nodes: vec![
                AstNode::Text("Hello ".to_string()),
                AstNode::Variable {
                    path: vec!["name".to_string()],
                    directives: vec![],
                    default: Some("World".to_string()),
                },
                AstNode::Text("!".to_string()),
            ],
        };
        
        let mut context = Context::new();
        context.insert("name", &"Luminess");
        
        let result = renderer.render(&template, &context).unwrap();
        assert_eq!(result, "Hello Luminess!");
    }
}
