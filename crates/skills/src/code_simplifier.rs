//! Code simplifier using AST-based skeletonization.
//!
//! Extracts function signatures, struct definitions, and other
//! important structural information from code without the implementation
//! details. This reduces token usage when providing context to LLMs.

use quote::ToTokens;
use syn::{
    parse_file, Attribute, Field, Fields, FnArg, ImplItem, Item, ItemEnum, ItemFn, ItemImpl,
    ItemMod, ItemStruct, ItemTrait, PatType, ReturnType, Signature, TraitItem, Type, Visibility,
};

/// Result of code simplification.
#[derive(Debug, Clone)]
pub struct SimplifiedCode {
    /// The skeleton code.
    pub skeleton: String,
    /// Number of items extracted.
    pub item_count: usize,
    /// Language of the source.
    pub language: String,
}

/// Simplify Rust source code to its structural skeleton.
///
/// Extracts:
/// - Function signatures (without bodies)
/// - Struct definitions (with field types)
/// - Enum definitions
/// - Trait definitions
/// - Impl blocks (method signatures only)
/// - Module structure
pub fn simplify_rust_code(source: &str) -> Result<SimplifiedCode, String> {
    let syntax = parse_file(source).map_err(|e| format!("Parse error: {}", e))?;

    let mut output = String::new();
    let mut item_count = 0;

    for item in &syntax.items {
        if let Some(simplified) = simplify_item(item) {
            output.push_str(&simplified);
            output.push_str("\n\n");
            item_count += 1;
        }
    }

    Ok(SimplifiedCode {
        skeleton: output.trim().to_string(),
        item_count,
        language: "rust".to_string(),
    })
}

/// Simplify a single item.
fn simplify_item(item: &Item) -> Option<String> {
    match item {
        Item::Fn(f) => Some(simplify_fn(f)),
        Item::Struct(s) => Some(simplify_struct(s)),
        Item::Enum(e) => Some(simplify_enum(e)),
        Item::Trait(t) => Some(simplify_trait(t)),
        Item::Impl(i) => Some(simplify_impl(i)),
        Item::Mod(m) => simplify_mod(m),
        Item::Use(_) => None, // Skip use statements
        Item::Const(c) => Some(format!(
            "{}const {}: {};",
            format_visibility(&c.vis),
            c.ident,
            c.ty.to_token_stream()
        )),
        Item::Static(s) => Some(format!(
            "{}static {}: {};",
            format_visibility(&s.vis),
            s.ident,
            s.ty.to_token_stream()
        )),
        Item::Type(t) => Some(format!(
            "{}type {} = {};",
            format_visibility(&t.vis),
            t.ident,
            t.ty.to_token_stream()
        )),
        _ => None,
    }
}

/// Simplify a function.
fn simplify_fn(f: &ItemFn) -> String {
    let attrs = format_attrs(&f.attrs);
    let vis = format_visibility(&f.vis);
    let sig = format_signature(&f.sig);

    format!("{}{}{} {{ ... }}", attrs, vis, sig)
}

/// Simplify a struct.
fn simplify_struct(s: &ItemStruct) -> String {
    let attrs = format_attrs(&s.attrs);
    let vis = format_visibility(&s.vis);
    let generics = s.generics.to_token_stream().to_string();

    let fields = match &s.fields {
        Fields::Named(named) => {
            let field_strs: Vec<String> = named.named.iter().map(format_field).collect();
            format!(" {{\n    {}\n}}", field_strs.join(",\n    "))
        }
        Fields::Unnamed(unnamed) => {
            let field_strs: Vec<String> =
                unnamed.unnamed.iter().map(|f| format_type(&f.ty)).collect();
            format!("({});", field_strs.join(", "))
        }
        Fields::Unit => ";".to_string(),
    };

    format!("{}{}struct {}{}{}", attrs, vis, s.ident, generics, fields)
}

/// Simplify an enum.
fn simplify_enum(e: &ItemEnum) -> String {
    let attrs = format_attrs(&e.attrs);
    let vis = format_visibility(&e.vis);
    let generics = e.generics.to_token_stream().to_string();

    let variants: Vec<String> = e
        .variants
        .iter()
        .map(|v| {
            let name = &v.ident;
            match &v.fields {
                Fields::Named(named) => {
                    let fields: Vec<String> = named
                        .named
                        .iter()
                        .map(|f| format!("{}: {}", f.ident.as_ref().unwrap(), format_type(&f.ty)))
                        .collect();
                    format!("{} {{ {} }}", name, fields.join(", "))
                }
                Fields::Unnamed(unnamed) => {
                    let fields: Vec<String> =
                        unnamed.unnamed.iter().map(|f| format_type(&f.ty)).collect();
                    format!("{}({})", name, fields.join(", "))
                }
                Fields::Unit => name.to_string(),
            }
        })
        .collect();

    format!(
        "{}{}enum {}{} {{\n    {}\n}}",
        attrs,
        vis,
        e.ident,
        generics,
        variants.join(",\n    ")
    )
}

/// Simplify a trait.
fn simplify_trait(t: &ItemTrait) -> String {
    let attrs = format_attrs(&t.attrs);
    let vis = format_visibility(&t.vis);
    let generics = t.generics.to_token_stream().to_string();

    let items: Vec<String> = t
        .items
        .iter()
        .filter_map(|item| match item {
            TraitItem::Fn(m) => Some(format!("    {};", format_signature(&m.sig))),
            TraitItem::Type(t) => Some(format!("    type {};", t.ident)),
            TraitItem::Const(c) => Some(format!(
                "    const {}: {};",
                c.ident,
                c.ty.to_token_stream()
            )),
            _ => None,
        })
        .collect();

    format!(
        "{}{}trait {}{} {{\n{}\n}}",
        attrs,
        vis,
        t.ident,
        generics,
        items.join("\n")
    )
}

/// Simplify an impl block.
fn simplify_impl(i: &ItemImpl) -> String {
    let generics = i.generics.to_token_stream().to_string();
    let self_ty = i.self_ty.to_token_stream().to_string();

    let trait_part = if let Some((_, path, _)) = &i.trait_ {
        format!("{} for ", path.to_token_stream())
    } else {
        String::new()
    };

    let items: Vec<String> = i
        .items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Fn(m) => {
                let vis = format_visibility(&m.vis);
                Some(format!("    {}{} {{ ... }}", vis, format_signature(&m.sig)))
            }
            ImplItem::Type(t) => Some(format!(
                "    type {} = {};",
                t.ident,
                t.ty.to_token_stream()
            )),
            ImplItem::Const(c) => Some(format!(
                "    const {}: {} = ...;",
                c.ident,
                c.ty.to_token_stream()
            )),
            _ => None,
        })
        .collect();

    format!(
        "impl{} {}{} {{\n{}\n}}",
        generics,
        trait_part,
        self_ty,
        items.join("\n")
    )
}

/// Simplify a module.
fn simplify_mod(m: &ItemMod) -> Option<String> {
    let vis = format_visibility(&m.vis);

    if let Some((_, items)) = &m.content {
        let inner: Vec<String> = items
            .iter()
            .filter_map(simplify_item)
            .map(|s| format!("    {}", s.replace('\n', "\n    ")))
            .collect();

        if inner.is_empty() {
            Some(format!("{}mod {};", vis, m.ident))
        } else {
            Some(format!(
                "{}mod {} {{\n{}\n}}",
                vis,
                m.ident,
                inner.join("\n\n")
            ))
        }
    } else {
        Some(format!("{}mod {};", vis, m.ident))
    }
}

/// Format visibility.
fn format_visibility(vis: &Visibility) -> String {
    match vis {
        Visibility::Public(_) => "pub ".to_string(),
        Visibility::Restricted(r) => format!("pub({}) ", r.path.to_token_stream()),
        Visibility::Inherited => String::new(),
    }
}

/// Format a function signature.
fn format_signature(sig: &Signature) -> String {
    let asyncness = if sig.asyncness.is_some() {
        "async "
    } else {
        ""
    };
    let unsafety = if sig.unsafety.is_some() {
        "unsafe "
    } else {
        ""
    };
    let name = &sig.ident;
    let generics = sig.generics.to_token_stream().to_string();

    let inputs: Vec<String> = sig
        .inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Receiver(r) => {
                let mutability = if r.mutability.is_some() { "mut " } else { "" };
                let reference = if r.reference.is_some() { "&" } else { "" };
                format!("{}{}self", reference, mutability)
            }
            FnArg::Typed(PatType { pat, ty, .. }) => {
                format!("{}: {}", pat.to_token_stream(), format_type(ty))
            }
        })
        .collect();

    let output = match &sig.output {
        ReturnType::Default => String::new(),
        ReturnType::Type(_, ty) => format!(" -> {}", format_type(ty)),
    };

    format!(
        "{}{}fn {}{}({}){}",
        asyncness,
        unsafety,
        name,
        generics,
        inputs.join(", "),
        output
    )
}

/// Format a struct field.
fn format_field(field: &Field) -> String {
    let vis = format_visibility(&field.vis);
    let name = field
        .ident
        .as_ref()
        .map(|i| i.to_string())
        .unwrap_or_default();
    let ty = format_type(&field.ty);

    format!("{}{}: {}", vis, name, ty)
}

/// Format a type (simplified).
fn format_type(ty: &Type) -> String {
    ty.to_token_stream().to_string()
}

/// Format doc attributes (only keep doc comments).
fn format_attrs(attrs: &[Attribute]) -> String {
    let doc_attrs: Vec<String> = attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .map(|a| a.to_token_stream().to_string())
        .collect();

    if doc_attrs.is_empty() {
        String::new()
    } else {
        format!("{}\n", doc_attrs.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simplify_function() {
        let code = r#"
            pub fn hello_world(name: &str, count: u32) -> String {
                format!("Hello, {}! Count: {}", name, count)
            }
        "#;

        let result = simplify_rust_code(code).unwrap();
        assert!(result.skeleton.contains("pub fn hello_world"));
        assert!(result.skeleton.contains("name: & str"));
        assert!(result.skeleton.contains("-> String"));
        assert!(result.skeleton.contains("{ ... }"));
        assert!(!result.skeleton.contains("format!"));
    }

    #[test]
    fn test_simplify_struct() {
        let code = r#"
            pub struct User {
                pub name: String,
                age: u32,
                email: Option<String>,
            }
        "#;

        let result = simplify_rust_code(code).unwrap();
        assert!(result.skeleton.contains("pub struct User"));
        assert!(result.skeleton.contains("pub name: String"));
        assert!(result.skeleton.contains("age: u32"));
    }

    #[test]
    fn test_simplify_enum() {
        let code = r#"
            pub enum Status {
                Active,
                Inactive,
                Pending { reason: String },
            }
        "#;

        let result = simplify_rust_code(code).unwrap();
        assert!(result.skeleton.contains("pub enum Status"));
        assert!(result.skeleton.contains("Active"));
        assert!(result.skeleton.contains("Pending { reason: String }"));
    }

    #[test]
    fn test_simplify_impl() {
        let code = r#"
            impl User {
                pub fn new(name: String) -> Self {
                    Self { name, age: 0, email: None }
                }

                fn get_age(&self) -> u32 {
                    self.age
                }
            }
        "#;

        let result = simplify_rust_code(code).unwrap();
        assert!(result.skeleton.contains("impl User"));
        assert!(result.skeleton.contains("pub fn new"));
        assert!(result.skeleton.contains("fn get_age"));
        assert!(result.skeleton.contains("&self"));
    }

    #[test]
    fn test_simplify_trait() {
        let code = r#"
            pub trait Greet {
                fn greet(&self) -> String;
                fn goodbye(&mut self);
            }
        "#;

        let result = simplify_rust_code(code).unwrap();
        assert!(result.skeleton.contains("pub trait Greet"));
        // Check for key parts, allowing for whitespace variations in tokenization
        assert!(result.skeleton.contains("fn greet"));
        assert!(result.skeleton.contains("-> String"));
        assert!(result.skeleton.contains("fn goodbye"));
        assert!(result.skeleton.contains("self"));
    }
}
