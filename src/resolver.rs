//! # Template Resolution
//! 
//! Resolves template files with smart fallbacks and configurable paths.

use crate::error::{LuminessError, Result};
use crate::LuminessConfig;
use std::path::PathBuf;

/// Resolves template files with smart fallback logic
pub struct TemplateResolver {
    template_paths: Vec<PathBuf>,
    extension: String,
}

impl TemplateResolver {
    /// Creates a new `TemplateResolver` from the provided configuration.
    pub fn new(config: &LuminessConfig) -> Result<Self> {
        let template_paths = config.template_paths
            .iter()
            .map(|p| PathBuf::from(p))
            .collect();
            
        Ok(Self {
            template_paths,
            extension: config.extension.clone(),
        })
    }
    
    /// Resolve a template name to a file path
    pub fn resolve_template(&self, name: &str) -> Result<PathBuf> {
        let filename = format!("{}.{}", name, self.extension);
        
        for path in &self.template_paths {
            let full_path = path.join(&filename);
            if full_path.exists() {
                return Ok(full_path);
            }
        }
        
        Err(LuminessError::template_not_found_with_paths(
            name,
            self.template_paths.iter().map(|p| p.display().to_string()).collect(),
        ))
    }
    
    /// Resolve header template with variant support
    /// 
    /// For @include_header("admin"), looks for:
    /// 1. header-admin.lmtp
    /// 2. header.lmtp (fallback)
    pub fn resolve_header(&self, variant: Option<&str>) -> Result<PathBuf> {
        if let Some(variant) = variant {
            // Try specific variant first
            let variant_name = format!("header-{}", variant);
            if let Ok(path) = self.resolve_template(&variant_name) {
                return Ok(path);
            }
        }
        
        // Fallback to default header
        self.resolve_template("header")
    }
    
    /// Resolve footer template with variant support
    /// 
    /// For @include_footer("minimal"), looks for:
    /// 1. footer-minimal.lmtp
    /// 2. footer.lmtp (fallback)
    pub fn resolve_footer(&self, variant: Option<&str>) -> Result<PathBuf> {
        if let Some(variant) = variant {
            // Try specific variant first
            let variant_name = format!("footer-{}", variant);
            if let Ok(path) = self.resolve_template(&variant_name) {
                return Ok(path);
            }
        }
        
        // Fallback to default footer
        self.resolve_template("footer")
    }
    
    /// Get all template paths being searched
    pub fn template_paths(&self) -> &[PathBuf] {
        &self.template_paths
    }
    
    /// Get the configured extension
    pub fn extension(&self) -> &str {
        &self.extension
    }
}
