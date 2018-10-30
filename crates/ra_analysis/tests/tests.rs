extern crate ra_analysis;
extern crate ra_editor;
extern crate ra_syntax;
extern crate relative_path;
extern crate rustc_hash;
extern crate test_utils;

use ra_syntax::TextRange;
use test_utils::{assert_eq_dbg, extract_offset};

use ra_analysis::{
    MockAnalysis,
    AnalysisChange, Analysis, CrateGraph, CrateId, FileId, FnDescriptor,
};

fn analysis(files: &[(&str, &str)]) -> Analysis {
    MockAnalysis::with_files(files).analysis()
}

fn get_signature(text: &str) -> (FnDescriptor, Option<usize>) {
    let (offset, code) = extract_offset(text);
    let code = code.as_str();

    let snap = analysis(&[("/lib.rs", code)]);

    snap.resolve_callable(FileId(1), offset).unwrap().unwrap()
}

#[test]
fn test_resolve_module() {
    let snap = analysis(&[("/lib.rs", "mod foo;"), ("/foo.rs", "")]);
    let symbols = snap.approximately_resolve_symbol(FileId(1), 4.into()).unwrap();
    assert_eq_dbg(
        r#"[(FileId(2), FileSymbol { name: "foo", node_range: [0; 0), kind: MODULE })]"#,
        &symbols,
    );

    let snap = analysis(&[("/lib.rs", "mod foo;"), ("/foo/mod.rs", "")]);
    let symbols = snap.approximately_resolve_symbol(FileId(1), 4.into()).unwrap();
    assert_eq_dbg(
        r#"[(FileId(2), FileSymbol { name: "foo", node_range: [0; 0), kind: MODULE })]"#,
        &symbols,
    );
}

#[test]
fn test_unresolved_module_diagnostic() {
    let snap = analysis(&[("/lib.rs", "mod foo;")]);
    let diagnostics = snap.diagnostics(FileId(1)).unwrap();
    assert_eq_dbg(
        r#"[Diagnostic {
            message: "unresolved module",
            range: [4; 7),
            fix: Some(SourceChange {
                label: "create module",
                source_file_edits: [],
                file_system_edits: [CreateFile { anchor: FileId(1), path: "../foo.rs" }],
                cursor_position: None }) }]"#,
        &diagnostics,
    );
}

#[test]
fn test_unresolved_module_diagnostic_no_diag_for_inline_mode() {
    let snap = analysis(&[("/lib.rs", "mod foo {}")]);
    let diagnostics = snap.diagnostics(FileId(1)).unwrap();
    assert_eq_dbg(r#"[]"#, &diagnostics);
}

#[test]
fn test_resolve_parent_module() {
    let snap = analysis(&[("/lib.rs", "mod foo;"), ("/foo.rs", "")]);
    let symbols = snap.parent_module(FileId(2)).unwrap();
    assert_eq_dbg(
        r#"[(FileId(1), FileSymbol { name: "foo", node_range: [0; 8), kind: MODULE })]"#,
        &symbols,
    );
}

#[test]
fn test_resolve_crate_root() {
    let mut host = MockAnalysis::with_files(
        &[("/lib.rs", "mod foo;"), ("/foo.rs", "")]
    ).analysis_host();
    let snap = host.analysis();
    assert!(snap.crate_for(FileId(2)).unwrap().is_empty());

    let crate_graph = {
        let mut g = CrateGraph::new();
        g.add_crate_root(FileId(1));
        g
    };
    let mut change = AnalysisChange::new();
    change.set_crate_graph(crate_graph);
    host.apply_change(change);
    let snap = host.analysis();

    assert_eq!(snap.crate_for(FileId(2)).unwrap(), vec![CrateId(0)],);
}

#[test]
fn test_fn_signature_two_args_first() {
    let (desc, param) = get_signature(
        r#"fn foo(x: u32, y: u32) -> u32 {x + y}
fn bar() { foo(<|>3, ); }"#,
    );

    assert_eq!(desc.name, "foo".to_string());
    assert_eq!(desc.params, vec!("x".to_string(), "y".to_string()));
    assert_eq!(desc.ret_type, Some("-> u32".into()));
    assert_eq!(param, Some(0));
}

#[test]
fn test_fn_signature_two_args_second() {
    let (desc, param) = get_signature(
        r#"fn foo(x: u32, y: u32) -> u32 {x + y}
fn bar() { foo(3, <|>); }"#,
    );

    assert_eq!(desc.name, "foo".to_string());
    assert_eq!(desc.params, vec!("x".to_string(), "y".to_string()));
    assert_eq!(desc.ret_type, Some("-> u32".into()));
    assert_eq!(param, Some(1));
}

#[test]
fn test_fn_signature_for_impl() {
    let (desc, param) = get_signature(
        r#"struct F; impl F { pub fn new() { F{}} }
fn bar() {let _ : F = F::new(<|>);}"#,
    );

    assert_eq!(desc.name, "new".to_string());
    assert_eq!(desc.params, Vec::<String>::new());
    assert_eq!(desc.ret_type, None);
    assert_eq!(param, None);
}

#[test]
fn test_fn_signature_for_method_self() {
    let (desc, param) = get_signature(
        r#"struct F;
impl F {
    pub fn new() -> F{
        F{}
    }

    pub fn do_it(&self) {}
}

fn bar() {
    let f : F = F::new();
    f.do_it(<|>);
}"#,
    );

    assert_eq!(desc.name, "do_it".to_string());
    assert_eq!(desc.params, vec!["&self".to_string()]);
    assert_eq!(desc.ret_type, None);
    assert_eq!(param, None);
}

#[test]
fn test_fn_signature_for_method_with_arg() {
    let (desc, param) = get_signature(
        r#"struct F;
impl F {
    pub fn new() -> F{
        F{}
    }

    pub fn do_it(&self, x: i32) {}
}

fn bar() {
    let f : F = F::new();
    f.do_it(<|>);
}"#,
    );

    assert_eq!(desc.name, "do_it".to_string());
    assert_eq!(desc.params, vec!["&self".to_string(), "x".to_string()]);
    assert_eq!(desc.ret_type, None);
    assert_eq!(param, Some(1));
}

#[test]
fn test_fn_signature_with_docs_simple() {
    let (desc, param) = get_signature(
        r#"
// test
fn foo(j: u32) -> u32 {
    j
}

fn bar() {
    let _ = foo(<|>);
}
"#,
    );

    assert_eq!(desc.name, "foo".to_string());
    assert_eq!(desc.params, vec!["j".to_string()]);
    assert_eq!(desc.ret_type, Some("-> u32".to_string()));
    assert_eq!(param, Some(0));
    assert_eq!(desc.label, "fn foo(j: u32) -> u32".to_string());
    assert_eq!(desc.doc, Some("test".into()));
}

#[test]
fn test_fn_signature_with_docs() {
    let (desc, param) = get_signature(
        r#"
/// Adds one to the number given.
///
/// # Examples
///
/// ```
/// let five = 5;
///
/// assert_eq!(6, my_crate::add_one(5));
/// ```
pub fn add_one(x: i32) -> i32 {
    x + 1
}

pub fn do() {
    add_one(<|>
}"#,
    );

    assert_eq!(desc.name, "add_one".to_string());
    assert_eq!(desc.params, vec!["x".to_string()]);
    assert_eq!(desc.ret_type, Some("-> i32".to_string()));
    assert_eq!(param, Some(0));
    assert_eq!(desc.label, "pub fn add_one(x: i32) -> i32".to_string());
    assert_eq!(desc.doc, Some(
r#"Adds one to the number given.

# Examples

```rust
let five = 5;

assert_eq!(6, my_crate::add_one(5));
```"#.into()));
}

#[test]
fn test_fn_signature_with_docs_impl() {
    let (desc, param) = get_signature(
        r#"
struct addr;
impl addr {
    /// Adds one to the number given.
    ///
    /// # Examples
    ///
    /// ```
    /// let five = 5;
    ///
    /// assert_eq!(6, my_crate::add_one(5));
    /// ```
    pub fn add_one(x: i32) -> i32 {
        x + 1
    }
}

pub fn do_it() {
    addr {};
    addr::add_one(<|>);
}"#);

    assert_eq!(desc.name, "add_one".to_string());
    assert_eq!(desc.params, vec!["x".to_string()]);
    assert_eq!(desc.ret_type, Some("-> i32".to_string()));
    assert_eq!(param, Some(0));
    assert_eq!(desc.label, "pub fn add_one(x: i32) -> i32".to_string());
    assert_eq!(desc.doc, Some(
r#"Adds one to the number given.

# Examples

```rust
let five = 5;

assert_eq!(6, my_crate::add_one(5));
```"#.into()));
}

#[test]
fn test_fn_signature_with_docs_from_actix() {
    let (desc, param) = get_signature(
        r#"
pub trait WriteHandler<E>
where
    Self: Actor,
    Self::Context: ActorContext,
{
    /// Method is called when writer emits error.
    ///
    /// If this method returns `ErrorAction::Continue` writer processing
    /// continues otherwise stream processing stops.
    fn error(&mut self, err: E, ctx: &mut Self::Context) -> Running {
        Running::Stop
    }

    /// Method is called when writer finishes.
    ///
    /// By default this method stops actor's `Context`.
    fn finished(&mut self, ctx: &mut Self::Context) {
        ctx.stop()
    }
}

pub fn foo() {
    WriteHandler r;
    r.finished(<|>);
}

"#);

    assert_eq!(desc.name, "finished".to_string());
    assert_eq!(desc.params, vec!["&mut self".to_string(), "ctx".to_string()]);
    assert_eq!(desc.ret_type, None);
    assert_eq!(param, Some(1));
    assert_eq!(desc.doc, Some(
r#"Method is called when writer finishes.

By default this method stops actor's `Context`."#.into()));
}


fn get_all_refs(text: &str) -> Vec<(FileId, TextRange)> {
    let (offset, code) = extract_offset(text);
    let code = code.as_str();

    let snap = analysis(&[("/lib.rs", code)]);

    snap.find_all_refs(FileId(1), offset).unwrap()
}

#[test]
fn test_find_all_refs_for_local() {
    let code = r#"
    fn main() {
        let mut i = 1;
        let j = 1;
        i = i<|> + j;

        {
            i = 0;
        }

        i = 5;
    }"#;

    let refs = get_all_refs(code);
    assert_eq!(refs.len(), 5);
}

#[test]
fn test_find_all_refs_for_param_inside() {
    let code = r#"
    fn foo(i : u32) -> u32 {
        i<|>
    }"#;

    let refs = get_all_refs(code);
    assert_eq!(refs.len(), 2);
}

#[test]
fn test_complete_crate_path() {
    let snap = analysis(&[
        ("/lib.rs", "mod foo; struct Spam;"),
        ("/foo.rs", "use crate::Sp"),
    ]);
    let completions = snap.completions(FileId(2), 13.into()).unwrap().unwrap();
    assert_eq_dbg(
        r#"[CompletionItem { label: "foo", lookup: None, snippet: None },
            CompletionItem { label: "Spam", lookup: None, snippet: None }]"#,
        &completions,
    );
}
