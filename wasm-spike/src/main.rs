//! Runtime probe: load a pnpm-lock.yaml, count entries, write it back out.
use pnpm_lockfile::Lockfile;
use std::path::Path;

fn main() {
    let dir = std::env::args().nth(1).expect("usage: wasm-spike <dir>");
    let lockfile = Lockfile::load_wanted_from_dir(Path::new(&dir))
        .expect("load")
        .expect("no pnpm-lock.yaml");
    println!("loaded: {} importers", lockfile.importers.len());
    lockfile.save_to_path(&Path::new(&dir).join("pnpm-lock.out.yaml")).expect("save");
    println!("saved");
    // Keep the resolver crate linked so we know it builds, not just checks.
    println!("resolver linked: NodeId is {} bytes", std::mem::size_of::<pnpm_resolving_deps_resolver::NodeId>());
}
