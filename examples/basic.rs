use luminess::{Luminess, Context};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Testing Luminess Template Engine 🚀");
    
    // Create engine with custom extension
    let mut engine = Luminess::new()
        .with_extension("lmtp")
        .with_template_path("templates");
    
    // Add a test template
    engine.add_template("welcome", r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>{{ title | UCASE, "Welcome" }}</title>
        </head>
        <body>
            <h1>{{ greeting | ESCAPE_HTML, "Hello World" }}</h1>
            
            {% if user.authenticated %}
                <p>Welcome back, {{ user.name | ESCAPE_HTML }}!</p>
                <p>Your role: {{ user.role | UCASE }}</p>
            {% else %}
                <p>Please log in to continue.</p>
            {% endif %}
            
            <h2>Recent Posts:</h2>
            <ul>
            {% for post in posts %}
                <li>
                    <strong>{{ post.title | ESCAPE_HTML }}</strong>
                    <em>({{ post.author }})</em>
                </li>
            {% endfor %}
            </ul>
            
            <footer>
                <p>{{ footer_text | ESCAPE_HTML, "© 2024 Luminess" }}</p>
            </footer>
        </body>
        </html>
    "#)?;
    
    // Create context with test data
    let mut context = Context::new();
    context.insert("title", &"luminess demo");
    context.insert("greeting", &"<Welcome to the fastest template engine>");
    
    // User data
    let user = json!({
        "authenticated": true,
        "name": "James",
        "role": "admin"
    });
    context.insert_object("user", &user);
    
    // Posts data
    let posts = json!([
        {
            "title": "Building Fast Templates",
            "author": "Developer"
        },
        {
            "title": "Rust Performance Tips", 
            "author": "Engineer"
        },
        {
            "title": "Template Security",
            "author": "Security Expert"
        }
    ]);
    context.insert_object("posts", &posts);
    
    // Render as HTML
    let (html_output, content_type) = engine.render_html("welcome", &context)?;
    
    println!("Content-Type: {}", content_type);
    println!("\n=== RENDERED HTML ===");
    println!("{}", html_output);
    
    // Test different output formats
    println!("\n=== TESTING OUTPUT FORMATS ===");
    
    // Simple template for format testing
    engine.add_template("simple", r#"{"message": "{{ msg | JSON_ENCODE }}", "user": "{{ user.name }}"}"#)?;    

    let mut simple_context = Context::new();
    simple_context.insert("msg", &"Hello \"World\"!");
    simple_context.insert_object("user", &json!({"name": "Tester"}));
    
    let (json_output, json_type) = engine.render_json("simple", &simple_context)?;
    println!("JSON ({}): {}", json_type, json_output);
    
    let (text_output, text_type) = engine.render_text("simple", &simple_context)?;
    println!("Text ({}): {}", text_type, text_output);
    
    println!("\n✅ All tests completed successfully!");
    println!("🎉 Luminess is working perfectly!");
    
    Ok(())
}
