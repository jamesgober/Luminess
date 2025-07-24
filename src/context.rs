//! # Template Context
//! 
//! Type-safe data management for template rendering.

use crate::error::{LuminessError, Result};
use serde_json::Value;
use std::collections::HashMap;

/// Template rendering context that holds all data
#[derive(Debug, Clone)]
pub struct Context {
    data: HashMap<String, Value>,
}

impl Context {
    /// Create a new empty context
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
    
    /// Create context from JSON value
    pub fn from_json(value: Value) -> Result<Self> {
        match value {
            Value::Object(map) => {
                let data = map.into_iter().collect();
                Ok(Self { data })
            }
            _ => Err(LuminessError::context("Context must be a JSON object")),
        }
    }
    
    /// Insert a serializable value
    pub fn insert<T: serde::Serialize + ?Sized>(&mut self, key: &str, value: &T) {
        if let Ok(json_value) = serde_json::to_value(value) {
            self.data.insert(key.to_string(), json_value);
        }
    }
    
    /// Insert a JSON object directly
    pub fn insert_object(&mut self, key: &str, value: &Value) {
        self.data.insert(key.to_string(), value.clone());
    }
    
    /// Get a value by path (e.g., "user.name")
    pub fn get_value(&self, path: &[String]) -> Option<&Value> {
        if path.is_empty() {
            return None;
        }
        
        let mut current = self.data.get(&path[0])?;
        
        for segment in path.iter().skip(1) {
            current = match current {
                Value::Object(map) => map.get(segment)?,
                Value::Array(arr) => {
                    let index: usize = segment.parse().ok()?;
                    arr.get(index)?
                }
                _ => return None,
            };
        }
        
        Some(current)
    }
    
    /// Check if a path exists and has a truthy value
    pub fn is_truthy(&self, path: &[String]) -> bool {
        match self.get_value(path) {
            Some(Value::Bool(b)) => *b,
            Some(Value::Null) => false,
            Some(Value::String(s)) => !s.is_empty(),
            Some(Value::Array(a)) => !a.is_empty(),
            Some(Value::Object(o)) => !o.is_empty(),
            Some(Value::Number(_)) => true,
            None => false,
        }
    }
    
    /// Get all top-level keys
    pub fn keys(&self) -> Vec<&String> {
        self.data.keys().collect()
    }
    
    /// Convert value to string for output
    pub fn value_to_string(&self, value: &Value) -> String {
        match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => String::new(),
            Value::Array(_) | Value::Object(_) => {
                serde_json::to_string(value).unwrap_or_else(|_| String::new())
            }
        }
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn basic_context_operations() {
        let mut context = Context::new();
        context.insert("name", &"John");
        context.insert("age", &25);
        
        let name_path = vec!["name".to_string()];
        let age_path = vec!["age".to_string()];
        
        assert_eq!(context.get_value(&name_path).unwrap().as_str().unwrap(), "John");
        assert_eq!(context.get_value(&age_path).unwrap().as_i64().unwrap(), 25);
    }
    
    #[test]
    fn nested_value_access() {
        let mut context = Context::new();
        let user = json!({
            "name": "John",
            "profile": {
                "email": "john@example.com",
                "settings": {
                    "theme": "dark"
                }
            }
        });
        context.insert_object("user", &user);
        
        let email_path = vec!["user".to_string(), "profile".to_string(), "email".to_string()];
        let theme_path = vec!["user".to_string(), "profile".to_string(), "settings".to_string(), "theme".to_string()];
        
        assert_eq!(context.get_value(&email_path).unwrap().as_str().unwrap(), "john@example.com");
        assert_eq!(context.get_value(&theme_path).unwrap().as_str().unwrap(), "dark");
    }
}
