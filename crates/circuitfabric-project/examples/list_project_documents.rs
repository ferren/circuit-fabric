//! Prints one project's indexed documents after loading the index (which also performs the
//! content-hash dedupe and persists it). Diagnostic tool, run manually:
//!
//! ```text
//! cargo run -p circuitfabric-project --example list_project_documents -- <project-root>
//! ```

fn main() {
    let Some(root) = std::env::args().nth(1) else {
        eprintln!("usage: list_project_documents <project-root>");
        return;
    };
    let storage = match circuitfabric_project::ProjectStorage::open(&root) {
        Ok(storage) => storage,
        Err(error) => {
            eprintln!("open failed: {error}");
            return;
        }
    };
    let documents = match storage.list_documents() {
        Ok(documents) => documents,
        Err(error) => {
            eprintln!("list failed: {error}");
            return;
        }
    };
    println!("project: {}", storage.manifest().project.id);
    println!("documents: {}", documents.len());
    for document in &documents {
        println!(
            "  {} | {} | {} | {}",
            document.id,
            document.original_file_name,
            &document.content_hash[..19],
            document.relative_path.display()
        );
    }
}
